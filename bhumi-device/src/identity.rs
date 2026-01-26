//! Identity management - stores keys in BHUMI_HOME

use std::fs;
use std::path::PathBuf;

use fastn_id52::{SecretKey, PublicKey};

/// Get BHUMI_HOME directory, creating it if needed
pub fn bhumi_home() -> PathBuf {
    let home = std::env::var("BHUMI_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .expect("no home directory")
                .join(".bhumi")
        });

    if !home.exists() {
        fs::create_dir_all(&home).expect("failed to create BHUMI_HOME");
    }

    home
}

/// Load or create device identity
pub fn load_or_create_identity() -> (SecretKey, PublicKey) {
    let home = bhumi_home();
    let key_path = home.join("identity.key");

    if key_path.exists() {
        // Load existing key
        let hex = fs::read_to_string(&key_path)
            .expect("failed to read identity.key");
        let secret_key: SecretKey = hex.trim().parse()
            .expect("failed to parse identity.key");
        let public_key = secret_key.public_key();

        println!("Loaded identity: {}", public_key);
        (secret_key, public_key)
    } else {
        // Generate new key
        let secret_key = SecretKey::generate();
        let public_key = secret_key.public_key();

        // Save to file
        fs::write(&key_path, secret_key.to_string())
            .expect("failed to write identity.key");

        println!("Created new identity: {}", public_key);
        println!("  Saved to: {}", key_path.display());

        (secret_key, public_key)
    }
}
