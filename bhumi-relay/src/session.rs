//! Session: handles a single device connection

use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

use bhumi_proto::{Frame, Hello, IAm, Send as SendMsg, Deliver, Ack, SendResult, MSG_I_AM, MSG_SEND, MSG_ACK};
use bhumi_proto::async_io::{read_frame, write_frame};
use fastn_id52::PublicKey;

use crate::router::{Router, PendingDelivery};

pub struct Session<S> {
    stream: S,
    router: Arc<Router>,
    nonce: u32,
    id52: Option<[u8; 32]>,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    pub fn new(stream: S, router: Arc<Router>, nonce: u32) -> Self {
        Self {
            stream,
            router,
            nonce,
            id52: None,
        }
    }

    pub async fn run(mut self) -> std::io::Result<()> {
        // Send HELLO
        let hello = Hello::new(self.nonce, 64 * 1024); // 64KB max payload
        write_frame(&mut self.stream, &Frame::hello(&hello)).await?;
        println!("  Sent HELLO (nonce=0x{:08x})", self.nonce);

        // Create channel for incoming deliveries
        let (tx, mut rx) = mpsc::channel::<PendingDelivery>(32);

        // Main loop: handle incoming frames and outgoing deliveries
        loop {
            tokio::select! {
                // Incoming frame from device
                frame_result = read_frame(&mut self.stream) => {
                    let frame = frame_result?;
                    if !self.handle_frame(frame, tx.clone()).await? {
                        break;
                    }
                }

                // Outgoing delivery to device
                Some(delivery) = rx.recv() => {
                    self.send_delivery(delivery).await?;
                }
            }
        }

        // Cleanup
        if let Some(id52) = &self.id52 {
            self.router.unregister(id52).await;
        }

        Ok(())
    }

    async fn handle_frame(
        &mut self,
        frame: Frame,
        sender: mpsc::Sender<PendingDelivery>,
    ) -> std::io::Result<bool> {
        match frame.msg_type {
            MSG_I_AM => {
                let i_am = IAm::from_bytes(&frame.payload)?;
                self.handle_i_am(i_am, sender).await?;
            }
            MSG_SEND => {
                let send = SendMsg::from_bytes(&frame.payload)?;
                self.handle_send(send).await?;
            }
            MSG_ACK => {
                let ack = Ack::from_bytes(&frame.payload)?;
                self.handle_ack(ack).await?;
            }
            other => {
                println!("  Unknown message type: 0x{:04x}", other);
            }
        }
        Ok(true)
    }

    async fn handle_i_am(
        &mut self,
        i_am: IAm,
        sender: mpsc::Sender<PendingDelivery>,
    ) -> std::io::Result<()> {
        // Verify signature: Sign(nonce || id52)
        let mut msg = Vec::with_capacity(4 + 32);
        msg.extend_from_slice(&self.nonce.to_be_bytes());
        msg.extend_from_slice(&i_am.id52);

        let public_key = PublicKey::from_bytes(&i_am.id52)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid id52: {e}")))?;

        let signature = fastn_id52::Signature::from_bytes(&i_am.signature)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid signature: {e}")))?;

        public_key.verify(&msg, &signature)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::PermissionDenied, "signature verification failed"))?;

        println!(
            "  I_AM verified: {} ({} commits, {} recent responses)",
            public_key,
            i_am.commits.len(),
            i_am.recent_responses.len()
        );

        // Unregister old identity if any
        if let Some(old_id52) = self.id52.take() {
            self.router.unregister(&old_id52).await;
        }

        // Convert recent_responses to format expected by router
        let recent_responses: Vec<_> = i_am.recent_responses
            .into_iter()
            .map(|r| (r.preimage, r.response))
            .collect();

        // Register new identity
        self.id52 = Some(i_am.id52);
        self.router.register(i_am.id52, i_am.commits, recent_responses, sender).await;

        Ok(())
    }

    async fn handle_send(&mut self, send: SendMsg) -> std::io::Result<()> {
        let to_id52_str = data_encoding::BASE32_DNSSEC.encode(&send.to_id52);
        println!(
            "  SEND to {} ({} bytes payload)",
            to_id52_str,
            send.payload.len()
        );

        let outcome = self.router.route_message(send.to_id52, send.preimage, send.payload).await;

        let status_str = match outcome.status {
            0 => "success",
            1 => "not connected",
            2 => "invalid preimage",
            3 => "timeout",
            4 => "disconnected",
            _ => "unknown",
        };
        println!("    -> {} ({} bytes response)", status_str, outcome.payload.len());

        // Send SEND_RESULT back to sender
        let result = SendResult {
            status: outcome.status,
            payload: outcome.payload,
        };
        write_frame(&mut self.stream, &Frame::send_result(&result)).await?;

        Ok(())
    }

    async fn handle_ack(&mut self, ack: Ack) -> std::io::Result<()> {
        println!("  ACK for msg_id={} ({} bytes)", ack.msg_id, ack.payload.len());
        self.router.handle_ack(ack.msg_id, ack.payload).await;
        Ok(())
    }

    async fn send_delivery(&mut self, delivery: PendingDelivery) -> std::io::Result<()> {
        let deliver = Deliver {
            msg_id: delivery.msg_id,
            payload: delivery.payload,
        };
        write_frame(&mut self.stream, &Frame::deliver(&deliver)).await?;
        println!("  Sent DELIVER (msg_id={})", delivery.msg_id);
        Ok(())
    }
}
