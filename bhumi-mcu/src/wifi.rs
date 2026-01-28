//! WiFi Abstraction Traits
//!
//! Traits for WiFi operations that MCU-specific crates implement.

/// WiFi network scan result
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub ssid: String,
    pub channel: u8,
    pub rssi: i8,
    pub auth_required: bool,
}

/// WiFi connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiStatus {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// IP address info
#[derive(Debug, Clone)]
pub struct IpInfo {
    pub ip: [u8; 4],
    pub gateway: [u8; 4],
    pub netmask: [u8; 4],
}

impl IpInfo {
    pub fn ip_str(&self) -> String {
        format!("{}.{}.{}.{}", self.ip[0], self.ip[1], self.ip[2], self.ip[3])
    }
}

/// Trait for WiFi operations
///
/// MCU-specific crates implement this trait using their WiFi stack.
pub trait Wifi {
    /// Error type for WiFi operations
    type Error;

    /// Scan for available networks
    fn scan(&mut self) -> Result<Vec<ScanResult>, Self::Error>;

    /// Connect to a WiFi network
    fn connect(&mut self, ssid: &str, password: &str) -> Result<(), Self::Error>;

    /// Disconnect from WiFi
    fn disconnect(&mut self) -> Result<(), Self::Error>;

    /// Get current connection status
    fn status(&self) -> WifiStatus;

    /// Get IP info (if connected)
    fn ip_info(&self) -> Option<IpInfo>;

    /// Check if connected
    fn is_connected(&self) -> bool {
        self.status() == WifiStatus::Connected
    }
}
