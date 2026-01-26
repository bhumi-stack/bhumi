//! Commit/preimage management for DoS protection

use std::fs;
use std::path::PathBuf;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

use crate::identity::bhumi_home;

/// A preimage and its corresponding commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreimageCommit {
    /// The secret preimage (share with trusted senders)
    pub preimage: [u8; 32],
    /// The commit = SHA256(preimage) (share with relay)
    pub commit: [u8; 32],
    /// Whether this preimage has been shared with someone
    pub shared: bool,
}

impl PreimageCommit {
    /// Generate a new random preimage and compute its commit
    pub fn generate() -> Self {
        let mut preimage = [0u8; 32];
        rand::Rng::fill(&mut rand::thread_rng(), &mut preimage);
        let commit: [u8; 32] = Sha256::digest(&preimage).into();

        Self {
            preimage,
            commit,
            shared: false,
        }
    }

    /// Format preimage as hex for sharing
    pub fn preimage_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.preimage)
    }

    /// Format commit as hex
    pub fn commit_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.commit)
    }
}

/// Manages preimages/commits for a device
#[derive(Debug, Serialize, Deserialize)]
pub struct CommitStore {
    preimages: Vec<PreimageCommit>,
}

impl CommitStore {
    fn store_path() -> PathBuf {
        bhumi_home().join("commits.json")
    }

    /// Load from disk or create new
    pub fn load_or_create(count: usize) -> Self {
        let path = Self::store_path();

        let mut store = if path.exists() {
            let data = fs::read_to_string(&path)
                .expect("failed to read commits.json");
            serde_json::from_str(&data)
                .expect("failed to parse commits.json")
        } else {
            Self { preimages: Vec::new() }
        };

        // Ensure we have enough commits
        while store.preimages.len() < count {
            store.preimages.push(PreimageCommit::generate());
        }

        store.save();
        store
    }

    /// Save to disk
    pub fn save(&self) {
        let path = Self::store_path();
        let data = serde_json::to_string_pretty(self)
            .expect("failed to serialize commits");
        fs::write(&path, data)
            .expect("failed to write commits.json");
    }

    /// Get all commits (for I_AM message)
    pub fn get_commits(&self) -> Vec<[u8; 32]> {
        self.preimages.iter().map(|p| p.commit).collect()
    }

    /// Get unshared preimages for display
    pub fn get_unshared(&self) -> Vec<&PreimageCommit> {
        self.preimages.iter().filter(|p| !p.shared).collect()
    }

    /// Mark a preimage as shared
    pub fn mark_shared(&mut self, index: usize) {
        if index < self.preimages.len() {
            self.preimages[index].shared = true;
            self.save();
        }
    }

    /// Print preimages for user to share
    pub fn print_preimages(&self, id52: &str) {
        println!("\n=== Preimages to share with senders ===");
        println!("Your id52: {}", id52);
        println!();

        for (i, pc) in self.preimages.iter().enumerate() {
            let status = if pc.shared { " (shared)" } else { "" };
            println!("[{}]{} preimage: {}", i, status, pc.preimage_hex());
        }

        println!("\nTo send a message to this device, the sender needs:");
        println!("  - Your id52 (above)");
        println!("  - One of the preimages (above)");
        println!("========================================\n");
    }
}
