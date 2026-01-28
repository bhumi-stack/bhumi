//! Persistent Storage Abstraction Traits
//!
//! Traits for non-volatile storage operations (WiFi credentials, device state, etc.)

/// Trait for persistent storage operations
///
/// MCU-specific crates implement this trait using their storage backend
/// (NVS for ESP32, flash for Pico, etc.)
pub trait Storage {
    /// Error type for storage operations
    type Error;

    /// Get WiFi credentials (SSID, password)
    fn get_wifi_credentials(&self) -> Result<Option<(String, String)>, Self::Error>;

    /// Save WiFi credentials
    fn set_wifi_credentials(&mut self, ssid: &str, password: &str) -> Result<(), Self::Error>;

    /// Clear WiFi credentials
    fn clear_wifi_credentials(&mut self) -> Result<(), Self::Error>;

    /// Get device secret key bytes (32 bytes)
    fn get_secret_key(&self) -> Result<Option<[u8; 32]>, Self::Error>;

    /// Save device secret key
    fn set_secret_key(&mut self, key: &[u8; 32]) -> Result<(), Self::Error>;

    /// Get device state as JSON bytes
    fn get_device_state(&self) -> Result<Option<Vec<u8>>, Self::Error>;

    /// Save device state as JSON bytes
    fn set_device_state(&mut self, state: &[u8]) -> Result<(), Self::Error>;

    /// Clear all device data (factory reset)
    fn clear_all(&mut self) -> Result<(), Self::Error>;

    /// Get a custom u8 value
    fn get_u8(&self, key: &str) -> Result<Option<u8>, Self::Error>;

    /// Set a custom u8 value
    fn set_u8(&mut self, key: &str, value: u8) -> Result<(), Self::Error>;
}
