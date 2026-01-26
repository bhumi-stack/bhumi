//! Router: maps id52 to connections, handles message routing and response caching

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, RwLock};

use bhumi_proto::{SEND_OK, SEND_ERR_NOT_CONNECTED, SEND_ERR_INVALID_PREIMAGE, SEND_ERR_TIMEOUT, SEND_ERR_DISCONNECTED};

/// Message to send to a connected device
pub struct PendingDelivery {
    pub msg_id: u32,
    pub payload: Vec<u8>,
}

/// Cached response for idempotent retry
struct CachedResponse {
    response: Vec<u8>,
    expires_at: Instant,
}

/// Per-device state
struct DeviceState {
    /// Valid commits for this device (SHA256 hashes)
    commits: HashSet<[u8; 32]>,
    /// Channel to send messages to this device
    sender: mpsc::Sender<PendingDelivery>,
}

/// Result of a send operation
pub struct SendOutcome {
    pub status: u8,
    pub payload: Vec<u8>,
}

/// Router manages device connections and message routing
pub struct Router {
    /// Map of id52 (public key bytes) to device state
    devices: RwLock<HashMap<[u8; 32], DeviceState>>,
    /// Global response cache (keyed by preimage)
    response_cache: RwLock<HashMap<[u8; 32], CachedResponse>>,
    /// Pending deliveries waiting for ACK (keyed by msg_id)
    /// Stores (preimage, response_tx) so we can cache response under correct preimage
    pending: RwLock<HashMap<u32, ([u8; 32], oneshot::Sender<Vec<u8>>)>>,
    /// Next message ID
    next_msg_id: RwLock<u32>,
    /// Response cache TTL
    cache_ttl: Duration,
}

impl Router {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            devices: RwLock::new(HashMap::new()),
            response_cache: RwLock::new(HashMap::new()),
            pending: RwLock::new(HashMap::new()),
            next_msg_id: RwLock::new(1),
            cache_ttl: Duration::from_secs(300), // 5 minutes
        })
    }

    /// Register a device with its commits and recent responses
    pub async fn register(
        &self,
        id52: [u8; 32],
        commits: Vec<[u8; 32]>,
        recent_responses: Vec<([u8; 32], Vec<u8>)>,
        sender: mpsc::Sender<PendingDelivery>,
    ) {
        let mut devices = self.devices.write().await;
        let commit_set: HashSet<[u8; 32]> = commits.into_iter().collect();

        println!(
            "  Router: registered {} with {} commits",
            data_encoding::BASE32_DNSSEC.encode(&id52),
            commit_set.len()
        );

        devices.insert(id52, DeviceState {
            commits: commit_set,
            sender,
        });

        // Populate response cache from recent responses
        if !recent_responses.is_empty() {
            let count = recent_responses.len();
            let mut cache = self.response_cache.write().await;
            let expires_at = Instant::now() + self.cache_ttl;
            for (preimage, response) in recent_responses {
                cache.insert(preimage, CachedResponse { response, expires_at });
            }
            println!("  Router: loaded {} recent responses into cache", count);
        }
    }

    /// Unregister a device
    pub async fn unregister(&self, id52: &[u8; 32]) {
        let mut devices = self.devices.write().await;
        devices.remove(id52);
        println!(
            "  Router: unregistered {}",
            data_encoding::BASE32_DNSSEC.encode(id52)
        );
    }

    /// Handle ACK from a recipient
    pub async fn handle_ack(&self, msg_id: u32, response: Vec<u8>) {
        // Get pending entry (includes preimage)
        let entry = {
            let mut pending = self.pending.write().await;
            pending.remove(&msg_id)
        };

        if let Some((preimage, tx)) = entry {
            // Cache the response under the preimage
            {
                let mut cache = self.response_cache.write().await;
                cache.insert(preimage, CachedResponse {
                    response: response.clone(),
                    expires_at: Instant::now() + self.cache_ttl,
                });
            }

            // Complete the pending send
            let _ = tx.send(response);
        }
    }

    /// Try to route a message to a device (synchronous - waits for response)
    pub async fn route_message(
        &self,
        to_id52: [u8; 32],
        preimage: [u8; 32],
        payload: Vec<u8>,
    ) -> SendOutcome {
        // 1. Check response cache first
        {
            let mut cache = self.response_cache.write().await;
            if let Some(cached) = cache.remove(&preimage) {
                if cached.expires_at > Instant::now() {
                    println!("    -> cached response");
                    return SendOutcome {
                        status: SEND_OK,
                        payload: cached.response,
                    };
                }
            }
        }

        // 2. Compute commit from preimage
        use sha2::{Sha256, Digest};
        let commit: [u8; 32] = Sha256::digest(&preimage).into();

        // 3. Check recipient and validate commit
        let (msg_id, sender) = {
            let mut devices = self.devices.write().await;

            let device = match devices.get_mut(&to_id52) {
                Some(d) => d,
                None => return SendOutcome {
                    status: SEND_ERR_NOT_CONNECTED,
                    payload: Vec::new(),
                },
            };

            // Check if commit is valid
            if !device.commits.remove(&commit) {
                return SendOutcome {
                    status: SEND_ERR_INVALID_PREIMAGE,
                    payload: Vec::new(),
                };
            }

            // Generate message ID
            let msg_id = {
                let mut id = self.next_msg_id.write().await;
                let current = *id;
                *id += 1;
                current
            };

            (msg_id, device.sender.clone())
        };

        // 4. Create response channel and register pending
        let (response_tx, response_rx) = oneshot::channel();

        // Register in pending map (before sending to avoid race)
        {
            let mut pending = self.pending.write().await;
            pending.insert(msg_id, (preimage, response_tx));
        }

        // Queue delivery
        let delivery = PendingDelivery { msg_id, payload };

        if sender.send(delivery).await.is_err() {
            // Remove from pending on failure
            let mut pending = self.pending.write().await;
            pending.remove(&msg_id);
            return SendOutcome {
                status: SEND_ERR_DISCONNECTED,
                payload: Vec::new(),
            };
        }

        // 5. Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(30), response_rx).await {
            Ok(Ok(response)) => {
                SendOutcome {
                    status: SEND_OK,
                    payload: response,
                }
            }
            Ok(Err(_)) => {
                // Channel closed (recipient disconnected)
                SendOutcome {
                    status: SEND_ERR_DISCONNECTED,
                    payload: Vec::new(),
                }
            }
            Err(_) => {
                // Timeout
                SendOutcome {
                    status: SEND_ERR_TIMEOUT,
                    payload: Vec::new(),
                }
            }
        }
    }

    /// Cleanup expired cache entries (call periodically)
    pub async fn cleanup_cache(&self) {
        let mut cache = self.response_cache.write().await;
        let now = Instant::now();
        cache.retain(|_, v| v.expires_at > now);
    }
}
