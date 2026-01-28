//! BLE GATT Service types and traits for Bhumi Device Provisioning
//!
//! Protocol constants (UUIDs, commands) are in bhumi_proto::ble.
//! This module provides MCU-specific types and traits.

// Re-export protocol constants for convenience
pub use bhumi_proto::ble::{SERVICE_UUID, WIFI_SSID_UUID, WIFI_PASS_UUID, COMMAND_UUID, STATUS_UUID, commands};

/// Pending WiFi credentials received via BLE
#[derive(Debug, Clone)]
pub struct PendingCredentials {
    pub ssid: String,
    pub password: String,
}

/// BLE provisioning command from client
#[derive(Debug)]
pub enum BleCommand {
    /// No pending command
    None,
    /// Reset device (unpaired only)
    Reset,
    /// Provision WiFi credentials (unpaired only)
    Provision(PendingCredentials),
    /// Authenticated reset with owner preimage
    ResetAuth([u8; 32]),
    /// Authenticated provision with owner preimage
    ProvisionAuth(PendingCredentials, [u8; 32]),
}

/// Device status for BLE status characteristic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceStatus {
    /// Device is unpaired, open for provisioning
    Unpaired,
    /// Device is paired, requires auth for provisioning
    Paired,
    /// WiFi is connected
    Connected,
    /// WiFi connection failed
    WifiFailed,
}

impl DeviceStatus {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            DeviceStatus::Unpaired => b"unpaired",
            DeviceStatus::Paired => b"paired",
            DeviceStatus::Connected => b"connected",
            DeviceStatus::WifiFailed => b"wifi_failed",
        }
    }
}

/// Trait for BLE GATT server implementations
///
/// MCU-specific crates implement this trait using their BLE stack.
pub trait BleServer {
    /// Error type for BLE operations
    type Error;

    /// Start BLE advertising with the given device name
    fn start_advertising(&mut self, device_name: &str) -> Result<(), Self::Error>;

    /// Stop BLE advertising
    fn stop_advertising(&mut self) -> Result<(), Self::Error>;

    /// Check for pending BLE command (non-blocking)
    fn poll_command(&mut self) -> Option<BleCommand>;

    /// Update the status characteristic value
    fn set_status(&mut self, status: DeviceStatus) -> Result<(), Self::Error>;
}

/// Validates a BLE command based on device pairing state
///
/// Returns error message if command is not allowed
pub fn validate_command(cmd: &BleCommand, is_paired: bool) -> Result<(), &'static str> {
    match cmd {
        BleCommand::None => Ok(()),
        BleCommand::Reset | BleCommand::Provision(_) => {
            if is_paired {
                Err("device is paired, authentication required")
            } else {
                Ok(())
            }
        }
        BleCommand::ResetAuth(_) | BleCommand::ProvisionAuth(_, _) => {
            // Auth commands are always allowed, validation happens later
            Ok(())
        }
    }
}
