//! Device state management using ESP32 NVS (Non-Volatile Storage)

use esp_idf_svc::nvs::{EspNvs, EspNvsPartition, NvsDefault};
use fastn_id52::{SecretKey, PublicKey};
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use bhumi_proto::{HandshakeInit, HandshakeComplete, HANDSHAKE_ACCEPTED, HANDSHAKE_REJECTED};
use data_encoding::BASE64URL_NOPAD;
use log::*;

const NVS_NAMESPACE: &str = "bhumi";
const KEY_SECRET: &str = "secret_key";
const KEY_STATE: &str = "state";
const KEY_LED_ON: &str = "led_on";

/// Peer role for access control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerRole {
    Owner,
    Writer,
    Reader,
}

/// Stored invite record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteRecord {
    pub alias: String,
    pub role: PeerRole,
}

/// Stored peer record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub alias: String,
    pub role: PeerRole,
    pub their_preimage: [u8; 32],  // Preimage they use to reach us
    pub our_preimage: [u8; 32],    // Preimage we use to reach them
    pub relay_url: Option<String>,
}

/// Persistent device state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredState {
    /// Pending invites: preimage -> invite record
    pub invites: Vec<([u8; 32], InviteRecord)>,
    /// Paired peers: id52 -> peer record
    pub peers: Vec<([u8; 32], PeerRecord)>,
}

/// Device state with keys and NVS access
pub struct DeviceState {
    secret_key: SecretKey,
    public_key: PublicKey,
    state: StoredState,
    nvs: EspNvs<NvsDefault>,
}

impl DeviceState {
    /// Load or create device state from NVS
    pub fn load(nvs_partition: &EspNvsPartition<NvsDefault>) -> Self {
        let mut nvs = EspNvs::new(nvs_partition.clone(), NVS_NAMESPACE, true)
            .expect("failed to open NVS namespace");

        // Load or generate secret key
        let secret_key = load_or_create_key(&mut nvs);
        let public_key = secret_key.public_key();

        // Load state
        let state = load_state(&nvs).unwrap_or_default();

        info!("Loaded state: {} invites, {} peers", state.invites.len(), state.peers.len());

        Self { secret_key, public_key, state, nvs }
    }

    pub fn id52(&self) -> String {
        self.public_key.to_string()
    }

    pub fn secret_key(&self) -> &SecretKey {
        &self.secret_key
    }

    pub fn is_paired(&self) -> bool {
        !self.state.peers.is_empty() || !self.state.invites.is_empty()
    }

    pub fn peer_count(&self) -> usize {
        self.state.peers.len()
    }

    pub fn invite_count(&self) -> usize {
        self.state.invites.len()
    }

    /// Get existing invite token (if any)
    pub fn get_invite_token(&self) -> Option<String> {
        self.state.invites.first().map(|(preimage, _)| {
            create_invite_token(&self.public_key.to_bytes(), preimage)
        })
    }

    /// Get all commits (SHA256 of preimages) for relay registration
    pub fn get_commits(&self) -> Vec<[u8; 32]> {
        let mut commits = Vec::new();

        // Invites
        for (preimage, _) in &self.state.invites {
            commits.push(sha256(preimage));
        }

        // Peers' preimages for reaching us
        for (_, peer) in &self.state.peers {
            commits.push(sha256(&peer.their_preimage));
        }

        commits
    }

    /// Create an owner invite and return the token
    pub fn create_owner_invite(&mut self) -> String {
        let preimage: [u8; 32] = rand::random();
        let invite = InviteRecord {
            alias: "owner".to_string(),
            role: PeerRole::Owner,
        };

        self.state.invites.push((preimage, invite));
        self.save();

        create_invite_token(&self.public_key.to_bytes(), &preimage)
    }

    /// Handle incoming handshake, returns (response_bytes, new_commit)
    pub fn handle_handshake(&mut self, msg: &super::connection::ReceivedMessage) -> Option<(Vec<u8>, Option<[u8; 32]>)> {
        let init = HandshakeInit::from_bytes(&msg.payload).ok()?;

        // Find the invite by preimage
        let invite_idx = self.state.invites.iter()
            .position(|(p, _)| sha256(p) == sha256(&msg.preimage))?;

        let (invite_preimage, invite) = self.state.invites.remove(invite_idx);

        // Generate our preimage for them
        let our_preimage: [u8; 32] = rand::random();

        // Create peer record
        let peer = PeerRecord {
            alias: invite.alias.clone(),
            role: invite.role,
            their_preimage: our_preimage,  // They'll use this to reach us
            our_preimage: init.preimage_for_peer, // We use this to reach them
            relay_url: if init.relay_url.is_empty() { None } else { Some(init.relay_url) },
        };

        self.state.peers.push((init.sender_id52, peer));
        self.save();

        let complete = HandshakeComplete {
            status: HANDSHAKE_ACCEPTED,
            preimage_for_peer: our_preimage,
            relay_url: String::new(), // ESP32 uses same relay
        };

        let new_commit = sha256(&our_preimage);
        Some((complete.to_bytes(), Some(new_commit)))
    }

    /// Generate rejection response
    pub fn reject_handshake(&self) -> Vec<u8> {
        HandshakeComplete {
            status: HANDSHAKE_REJECTED,
            preimage_for_peer: [0u8; 32],
            relay_url: String::new(),
        }.to_bytes()
    }

    /// Look up peer by preimage commit
    pub fn lookup_preimage(&self, preimage: &[u8; 32]) -> Option<(&[u8; 32], &PeerRecord)> {
        let commit = sha256(preimage);
        for (id52, peer) in &self.state.peers {
            if sha256(&peer.their_preimage) == commit {
                return Some((id52, peer));
            }
        }
        None
    }

    /// Consume preimage and generate new one, returns (new_preimage, new_commit)
    pub fn renew_preimage(&mut self, peer_id52: &[u8; 32], old_preimage: &[u8; 32]) -> Option<([u8; 32], [u8; 32])> {
        let old_commit = sha256(old_preimage);

        for (id52, peer) in &mut self.state.peers {
            if id52 == peer_id52 && sha256(&peer.their_preimage) == old_commit {
                let new_preimage: [u8; 32] = rand::random();
                peer.their_preimage = new_preimage;
                self.save();
                return Some((new_preimage, sha256(&new_preimage)));
            }
        }
        None
    }

    fn save(&mut self) {
        let data = serde_json::to_vec(&self.state).expect("failed to serialize state");
        self.nvs.set_blob(KEY_STATE, &data).expect("failed to save state to NVS");
    }

    /// Load LED state from NVS (returns false if not set)
    pub fn load_led_state(&self) -> bool {
        self.nvs.get_u8(KEY_LED_ON).ok().flatten().unwrap_or(0) != 0
    }

    /// Save LED state to NVS
    pub fn save_led_state(&mut self, on: bool) {
        let _ = self.nvs.set_u8(KEY_LED_ON, if on { 1 } else { 0 });
    }
}

fn load_or_create_key(nvs: &mut EspNvs<NvsDefault>) -> SecretKey {
    let mut buf = [0u8; 32];

    match nvs.get_blob(KEY_SECRET, &mut buf) {
        Ok(Some(data)) => {
            info!("Loaded existing secret key from NVS ({} bytes)", data.len());
            SecretKey::from_bytes(&buf)
        }
        _ => {
            info!("Generating new secret key");
            let key = SecretKey::generate();
            nvs.set_blob(KEY_SECRET, &key.to_bytes()).expect("failed to save key to NVS");
            key
        }
    }
}

fn load_state(nvs: &EspNvs<NvsDefault>) -> Option<StoredState> {
    let mut buf = [0u8; 4096];
    match nvs.get_blob(KEY_STATE, &mut buf) {
        Ok(Some(data)) => {
            serde_json::from_slice(data).ok()
        }
        _ => None,
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn create_invite_token(id52_bytes: &[u8; 32], preimage: &[u8; 32]) -> String {
    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(id52_bytes);
    combined.extend_from_slice(preimage);
    BASE64URL_NOPAD.encode(&combined)
}
