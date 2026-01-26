//! Device state management - invites, pending peers, and established peers

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Helper: convert [u8; 32] to hex string for JSON keys
fn bytes_to_hex(bytes: &[u8; 32]) -> String {
    data_encoding::HEXLOWER.encode(bytes)
}

/// Helper: convert hex string back to [u8; 32]
fn hex_to_bytes(hex: &str) -> Option<[u8; 32]> {
    let bytes = data_encoding::HEXLOWER.decode(hex.as_bytes()).ok()?;
    bytes.try_into().ok()
}

/// Role of a peer - determines what commands they can execute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerRole {
    /// Owner: full control, can manage invites
    Owner,
    /// Writer: can read and modify device state
    Writer,
    /// Reader: can only read device state
    Reader,
}

impl Default for PeerRole {
    fn default() -> Self {
        Self::Reader
    }
}

/// Pending invite I created, awaiting handshake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteRecord {
    pub alias: String,
    #[serde(with = "hex_bytes")]
    pub preimage: [u8; 32],
    #[serde(default)]
    pub role: PeerRole,
    pub created_at: u64,
}

/// Peer I'm trying to connect to (received their invite, handshake not complete)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPeerRecord {
    pub alias: String,
    #[serde(with = "hex_bytes")]
    pub their_id52: [u8; 32],
    #[serde(with = "hex_bytes")]
    pub their_preimage: [u8; 32],  // from invite token, for HANDSHAKE_INIT
    #[serde(with = "hex_bytes")]
    pub my_preimage: [u8; 32],     // I generated, for them to reply
    pub relay_url: Option<String>,
    pub created_at: u64,
}

/// Established peer - bidirectional communication possible
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerRecord {
    pub alias: String,
    #[serde(default)]
    pub role: PeerRole,
    pub last_known_relay: Option<String>,
    pub last_contacted: u64,

    /// Preimages I've issued to this peer (they use to contact me)
    #[serde(with = "hex_bytes_vec")]
    pub issued_preimages: Vec<[u8; 32]>,

    /// Preimage I use to contact them (they issued to me)
    #[serde(with = "hex_bytes_opt")]
    pub their_preimage: Option<[u8; 32]>,
}

/// Serializable device state using hex strings for byte array keys
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SerializableState {
    invites: HashMap<String, InviteRecord>,
    pending_peers: HashMap<String, PendingPeerRecord>,
    peers: HashMap<String, PeerRecord>,
}

/// Device state - persisted to disk
#[derive(Debug, Clone, Default)]
pub struct DeviceState {
    /// Invites I created, keyed by preimage for fast lookup on incoming HANDSHAKE_INIT
    pub invites: HashMap<[u8; 32], InviteRecord>,

    /// Peers I'm trying to connect to, keyed by their id52
    pub pending_peers: HashMap<[u8; 32], PendingPeerRecord>,

    /// Established peers, keyed by their id52
    pub peers: HashMap<[u8; 32], PeerRecord>,
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&data_encoding::HEXLOWER.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where D: Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        let bytes = data_encoding::HEXLOWER.decode(s.as_bytes())
            .map_err(serde::de::Error::custom)?;
        bytes.try_into().map_err(|_| serde::de::Error::custom("expected 32 bytes"))
    }
}

mod hex_bytes_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(opt: &Option<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match opt {
            Some(bytes) => serializer.serialize_some(&data_encoding::HEXLOWER.encode(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<[u8; 32]>, D::Error>
    where D: Deserializer<'de> {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => {
                let bytes = data_encoding::HEXLOWER.decode(s.as_bytes())
                    .map_err(serde::de::Error::custom)?;
                let arr: [u8; 32] = bytes.try_into()
                    .map_err(|_| serde::de::Error::custom("expected 32 bytes"))?;
                Ok(Some(arr))
            }
            None => Ok(None),
        }
    }
}

mod hex_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(vec: &Vec<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(vec.len()))?;
        for bytes in vec {
            seq.serialize_element(&data_encoding::HEXLOWER.encode(bytes))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<[u8; 32]>, D::Error>
    where D: Deserializer<'de> {
        let strings: Vec<String> = Vec::deserialize(deserializer)?;
        strings.into_iter().map(|s| {
            let bytes = data_encoding::HEXLOWER.decode(s.as_bytes())
                .map_err(serde::de::Error::custom)?;
            bytes.try_into().map_err(|_| serde::de::Error::custom("expected 32 bytes"))
        }).collect()
    }
}

impl DeviceState {
    /// Load state from file or create empty
    pub fn load(path: &PathBuf) -> Self {
        if path.exists() {
            let data = fs::read_to_string(path).expect("failed to read state file");
            let ser: SerializableState = serde_json::from_str(&data).expect("failed to parse state file");

            // Convert from serializable format
            let invites = ser.invites.into_iter()
                .filter_map(|(k, v)| hex_to_bytes(&k).map(|key| (key, v)))
                .collect();
            let pending_peers = ser.pending_peers.into_iter()
                .filter_map(|(k, v)| hex_to_bytes(&k).map(|key| (key, v)))
                .collect();
            let peers = ser.peers.into_iter()
                .filter_map(|(k, v)| hex_to_bytes(&k).map(|key| (key, v)))
                .collect();

            Self { invites, pending_peers, peers }
        } else {
            Self::default()
        }
    }

    /// Save state to file
    pub fn save(&self, path: &PathBuf) {
        // Convert to serializable format
        let ser = SerializableState {
            invites: self.invites.iter()
                .map(|(k, v)| (bytes_to_hex(k), v.clone()))
                .collect(),
            pending_peers: self.pending_peers.iter()
                .map(|(k, v)| (bytes_to_hex(k), v.clone()))
                .collect(),
            peers: self.peers.iter()
                .map(|(k, v)| (bytes_to_hex(k), v.clone()))
                .collect(),
        };

        let data = serde_json::to_string_pretty(&ser).expect("failed to serialize state");
        fs::write(path, data).expect("failed to write state file");
    }

    /// Create a new invite for a peer with a specific role
    pub fn create_invite(&mut self, alias: &str, role: PeerRole) -> (InviteRecord, [u8; 32]) {
        let preimage = random_bytes();
        let commit = sha256(&preimage);
        let now = current_timestamp();

        let record = InviteRecord {
            alias: alias.to_string(),
            preimage,
            role,
            created_at: now,
        };

        self.invites.insert(preimage, record.clone());
        (record, commit)
    }

    /// Accept an invite token (parse and store in pending_peers)
    pub fn accept_invite(&mut self, their_id52: [u8; 32], their_preimage: [u8; 32], alias: &str) -> ([u8; 32], [u8; 32]) {
        let my_preimage = random_bytes();
        let my_commit = sha256(&my_preimage);
        let now = current_timestamp();

        let record = PendingPeerRecord {
            alias: alias.to_string(),
            their_id52,
            their_preimage,
            my_preimage,
            relay_url: None,
            created_at: now,
        };

        self.pending_peers.insert(their_id52, record);
        (my_preimage, my_commit)
    }

    /// Complete handshake as invite creator (received HANDSHAKE_INIT)
    /// Returns new preimage to send in HANDSHAKE_COMPLETE
    pub fn complete_handshake_as_inviter(
        &mut self,
        preimage: &[u8; 32],
        peer_id52: [u8; 32],
        peer_preimage: [u8; 32],
        peer_relay: Option<String>,
    ) -> Option<([u8; 32], [u8; 32])> {
        // Look up invite
        let invite = self.invites.remove(preimage)?;

        // Generate new preimage for peer's next message
        let new_preimage = random_bytes();
        let new_commit = sha256(&new_preimage);

        // Create peer record with role from invite
        let peer = PeerRecord {
            alias: invite.alias,
            role: invite.role,
            last_known_relay: peer_relay,
            last_contacted: current_timestamp(),
            issued_preimages: vec![new_preimage],
            their_preimage: Some(peer_preimage),
        };

        self.peers.insert(peer_id52, peer);
        Some((new_preimage, new_commit))
    }

    /// Complete handshake as invite acceptor (received HANDSHAKE_COMPLETE)
    pub fn complete_handshake_as_acceptor(
        &mut self,
        peer_id52: &[u8; 32],
        peer_preimage: [u8; 32],
        peer_relay: Option<String>,
    ) -> bool {
        // Look up pending peer
        let pending = match self.pending_peers.remove(peer_id52) {
            Some(p) => p,
            None => return false,
        };

        // Create peer record (role is not relevant for acceptor - the device stores our role)
        let peer = PeerRecord {
            alias: pending.alias,
            role: PeerRole::Reader, // default, not used on acceptor side
            last_known_relay: peer_relay,
            last_contacted: current_timestamp(),
            issued_preimages: vec![pending.my_preimage],
            their_preimage: Some(peer_preimage),
        };

        self.peers.insert(*peer_id52, peer);
        true
    }

    /// Get all commits to register with relay
    pub fn get_all_commits(&self) -> Vec<[u8; 32]> {
        let mut commits = Vec::new();

        // Commits from invites
        for invite in self.invites.values() {
            commits.push(sha256(&invite.preimage));
        }

        // Commits from pending peers (our preimages)
        for pending in self.pending_peers.values() {
            commits.push(sha256(&pending.my_preimage));
        }

        // Commits from established peers (issued preimages)
        for peer in self.peers.values() {
            for preimage in &peer.issued_preimages {
                commits.push(sha256(preimage));
            }
        }

        commits
    }

    /// Look up who sent us a message based on preimage
    pub fn lookup_preimage(&self, preimage: &[u8; 32]) -> Option<PreimageLookup> {
        // Check invites (HANDSHAKE_INIT)
        if let Some(invite) = self.invites.get(preimage) {
            return Some(PreimageLookup::Invite(invite.clone()));
        }

        // Check pending peers (shouldn't receive messages here, but check anyway)
        // Actually pending peers use their_preimage to SEND, not receive

        // Check established peers
        for (id52, peer) in &self.peers {
            if peer.issued_preimages.contains(preimage) {
                return Some(PreimageLookup::Peer(*id52, peer.clone()));
            }
        }

        None
    }

    /// Consume a preimage from a peer and generate a new one
    pub fn consume_and_renew_preimage(&mut self, peer_id52: &[u8; 32], old_preimage: &[u8; 32]) -> Option<([u8; 32], [u8; 32])> {
        let peer = self.peers.get_mut(peer_id52)?;

        // Remove old preimage
        peer.issued_preimages.retain(|p| p != old_preimage);

        // Generate new preimage
        let new_preimage = random_bytes();
        let new_commit = sha256(&new_preimage);
        peer.issued_preimages.push(new_preimage);

        peer.last_contacted = current_timestamp();

        Some((new_preimage, new_commit))
    }

    /// Get a peer's preimage for sending
    pub fn get_peer_preimage(&self, peer_id52: &[u8; 32]) -> Option<[u8; 32]> {
        self.peers.get(peer_id52)?.their_preimage
    }

    /// Update peer's preimage after receiving a response
    pub fn update_peer_preimage(&mut self, peer_id52: &[u8; 32], new_preimage: [u8; 32]) {
        if let Some(peer) = self.peers.get_mut(peer_id52) {
            peer.their_preimage = Some(new_preimage);
            peer.last_contacted = current_timestamp();
        }
    }

    /// Find peer by alias
    pub fn find_peer_by_alias(&self, alias: &str) -> Option<([u8; 32], &PeerRecord)> {
        self.peers.iter()
            .find(|(_, p)| p.alias == alias)
            .map(|(id, p)| (*id, p))
    }
}

/// Result of preimage lookup
pub enum PreimageLookup {
    Invite(InviteRecord),
    Peer([u8; 32], PeerRecord),
}

/// Generate 32 random bytes
fn random_bytes() -> [u8; 32] {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

/// SHA256 hash
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    Sha256::digest(data).into()
}

/// Current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Create invite token from id52 and preimage
pub fn create_invite_token(id52: &[u8; 32], preimage: &[u8; 32]) -> String {
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(id52);
    data.extend_from_slice(preimage);
    data_encoding::BASE64URL_NOPAD.encode(&data)
}

/// Parse invite token
pub fn parse_invite_token(token: &str) -> Result<([u8; 32], [u8; 32]), &'static str> {
    let data = data_encoding::BASE64URL_NOPAD
        .decode(token.as_bytes())
        .map_err(|_| "invalid base64url")?;

    if data.len() != 64 {
        return Err("token must be 64 bytes");
    }

    let id52: [u8; 32] = data[0..32].try_into().unwrap();
    let preimage: [u8; 32] = data[32..64].try_into().unwrap();

    Ok((id52, preimage))
}
