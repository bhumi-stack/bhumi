pub fn create_key(home: &str) {
    let key = fastn_id52::SecretKey::generate();
    let path = std::path::PathBuf::from(home).join("hub.secret");

    if path.exists() {
        eprintln!("Key already exists at {}", path.display());
        std::process::exit(1);
    }

    std::fs::write(&path, key.to_string()).unwrap_or_else(|e| {
        eprintln!("Failed to write key to {}: {e}", path.display());
        std::process::exit(1);
    });

    println!("Created key at {}", path.display());
    println!("Public ID: {}", key.id52());
}

#[derive(Debug, thiserror::Error)]
pub enum ReadKeyError {
    #[error("secret key file not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("failed to read secret key file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid secret key format: {0}")]
    Parse(String),
}

pub fn read_key(home: &str) -> Result<fastn_id52::SecretKey, ReadKeyError> {
    let path = std::path::PathBuf::from(home).join("hub.secret");

    if !path.exists() {
        return Err(ReadKeyError::NotFound(path));
    }

    let content = std::fs::read_to_string(&path)?;
    let content = content.trim();

    content
        .parse::<fastn_id52::SecretKey>()
        .map_err(|e| ReadKeyError::Parse(e.to_string()))
}
