//! BLE GATT Service Protocol Constants for Bhumi Device Provisioning
//!
//! This module defines the BLE service UUIDs and command bytes used for
//! WiFi provisioning and device management over BLE.

/// BLE Service UUID: b40e1000-5e7c-1c3e-0000-000000000000
pub const SERVICE_UUID: &str = "b40e1000-5e7c-1c3e-0000-000000000000";

/// WiFi SSID Characteristic UUID (write)
pub const WIFI_SSID_UUID: &str = "b40e1001-5e7c-1c3e-0000-000000000000";

/// WiFi Password Characteristic UUID (write)
pub const WIFI_PASS_UUID: &str = "b40e1002-5e7c-1c3e-0000-000000000000";

/// Command Characteristic UUID (write)
pub const COMMAND_UUID: &str = "b40e1003-5e7c-1c3e-0000-000000000000";

/// Status Characteristic UUID (read/notify)
pub const STATUS_UUID: &str = "b40e1004-5e7c-1c3e-0000-000000000000";

/// BLE Command bytes
pub mod commands {
    /// Reset device - clears all settings (WiFi, keys, peers)
    /// Only allowed for unpaired devices or with owner preimage
    pub const RESET: u8 = 0x01;

    /// Provision WiFi - saves SSID/password and restarts
    /// Only allowed for unpaired devices
    pub const PROVISION: u8 = 0x02;

    /// Authenticated reset - requires 32-byte preimage after command byte
    /// For paired devices, owner must provide valid preimage
    pub const RESET_AUTH: u8 = 0x11;

    /// Authenticated provision - requires 32-byte preimage after command byte
    pub const PROVISION_AUTH: u8 = 0x12;
}
