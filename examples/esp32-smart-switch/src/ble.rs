//! BLE provisioning service for WiFi credentials and device reset
//!
//! Runs a GATT server that allows BLE clients to:
//! - Send WiFi SSID and password
//! - Trigger device reset (clear all settings)
//!
//! Uses UUIDs and commands from bhumi_mcu::ble protocol.

use esp32_nimble::{uuid128, BLEDevice, NimbleProperties, utilities::BleUuid};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log::*;
use std::sync::{Arc, Mutex};

// Use command constants from bhumi-mcu
use bhumi_mcu::ble::commands;

// BLE Service and Characteristic UUIDs
// These must match bhumi_mcu::ble::{SERVICE_UUID, WIFI_SSID_UUID, etc.}
// We use uuid128! macro for compile-time generation of BleUuid
const SERVICE_UUID: BleUuid = uuid128!("b40e1000-5e7c-1c3e-0000-000000000000");
const WIFI_SSID_UUID: BleUuid = uuid128!("b40e1001-5e7c-1c3e-0000-000000000000");
const WIFI_PASS_UUID: BleUuid = uuid128!("b40e1002-5e7c-1c3e-0000-000000000000");
const COMMAND_UUID: BleUuid = uuid128!("b40e1003-5e7c-1c3e-0000-000000000000");
const STATUS_UUID: BleUuid = uuid128!("b40e1004-5e7c-1c3e-0000-000000000000");

// NVS keys for WiFi credentials
const KEY_WIFI_SSID: &str = "wifi_ssid";
const KEY_WIFI_PASS: &str = "wifi_pass";

/// Pending WiFi credentials received via BLE
pub struct PendingCredentials {
    pub ssid: String,
    pub password: String,
}

/// BLE provisioning commands
pub enum BleCommand {
    None,
    Reset,
    Provision(PendingCredentials),
}

/// Shared state between BLE callbacks and main loop
pub struct BleState {
    ssid: String,
    password: String,
    command: BleCommand,
}

impl BleState {
    fn new() -> Self {
        Self {
            ssid: String::new(),
            password: String::new(),
            command: BleCommand::None,
        }
    }
}

/// Start the BLE GATT server for provisioning
/// Returns a handle to check for pending commands
pub fn start_ble_server(device_name: &str) -> Arc<Mutex<BleState>> {
    let state = Arc::new(Mutex::new(BleState::new()));

    let ble_device = BLEDevice::take();

    // Set the device name (this is what shows up in BLE scans)
    BLEDevice::set_device_name(device_name).expect("Failed to set device name");

    let server = ble_device.get_server();

    // Set connection callbacks
    server.on_connect(|server, desc| {
        info!("BLE client connected");
        // Update connection parameters for better performance
        let _ = server.update_conn_params(desc.conn_handle(), 24, 48, 0, 60);
    });

    server.on_disconnect(|_desc, _reason| {
        info!("BLE client disconnected");
    });

    // Create the provisioning service
    let service = server.create_service(SERVICE_UUID);

    // WiFi SSID characteristic (write only)
    let ssid_state = state.clone();
    let ssid_char = service.lock().create_characteristic(
        WIFI_SSID_UUID,
        NimbleProperties::WRITE,
    );
    ssid_char.lock().on_write(move |args| {
        if let Ok(ssid) = std::str::from_utf8(args.recv_data()) {
            info!("BLE: Received SSID: {}", ssid);
            if let Ok(mut s) = ssid_state.lock() {
                s.ssid = ssid.to_string();
            }
        }
    });

    // WiFi Password characteristic (write only)
    let pass_state = state.clone();
    let pass_char = service.lock().create_characteristic(
        WIFI_PASS_UUID,
        NimbleProperties::WRITE,
    );
    pass_char.lock().on_write(move |args| {
        info!("BLE: Received password ({} bytes)", args.recv_data().len());
        if let Ok(pass) = std::str::from_utf8(args.recv_data()) {
            if let Ok(mut s) = pass_state.lock() {
                s.password = pass.to_string();
            }
        }
    });

    // Command characteristic (write only)
    let cmd_state = state.clone();
    let cmd_char = service.lock().create_characteristic(
        COMMAND_UUID,
        NimbleProperties::WRITE,
    );
    cmd_char.lock().on_write(move |args| {
        let data = args.recv_data();
        if !data.is_empty() {
            match data[0] {
                commands::RESET => {
                    info!("BLE: Reset command received");
                    if let Ok(mut s) = cmd_state.lock() {
                        s.command = BleCommand::Reset;
                    }
                }
                commands::PROVISION => {
                    info!("BLE: Provision command received");
                    if let Ok(mut s) = cmd_state.lock() {
                        if !s.ssid.is_empty() && !s.password.is_empty() {
                            let creds = PendingCredentials {
                                ssid: s.ssid.clone(),
                                password: s.password.clone(),
                            };
                            s.command = BleCommand::Provision(creds);
                        } else {
                            warn!("BLE: Provision command but SSID/password not set");
                        }
                    }
                }
                _ => {
                    warn!("BLE: Unknown command: 0x{:02x}", data[0]);
                }
            }
        }
    });

    // Status characteristic (read/notify)
    let status_char = service.lock().create_characteristic(
        STATUS_UUID,
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    status_char.lock().set_value(b"ready");

    // Start advertising
    let advertising = ble_device.get_advertising();
    advertising.lock()
        .set_data(
            esp32_nimble::BLEAdvertisementData::new()
                .name(device_name)
                .add_service_uuid(SERVICE_UUID)
        )
        .expect("Failed to set advertising data");

    advertising.lock().start().expect("Failed to start BLE advertising");
    info!("BLE advertising started as '{}'", device_name);

    state
}

/// Check for pending BLE commands and handle them
/// Returns true if device should restart
pub fn check_ble_command(state: &Arc<Mutex<BleState>>, nvs: &mut EspNvs<NvsDefault>) -> bool {
    let command = {
        let mut s = match state.lock() {
            Ok(s) => s,
            Err(_) => return false,
        };
        std::mem::replace(&mut s.command, BleCommand::None)
    };

    match command {
        BleCommand::None => false,
        BleCommand::Reset => {
            info!("Executing device reset...");
            reset_device(nvs);
            true // Restart required
        }
        BleCommand::Provision(creds) => {
            info!("Saving WiFi credentials: SSID={}", creds.ssid);
            save_wifi_credentials(nvs, &creds.ssid, &creds.password);
            true // Restart required
        }
    }
}

/// Save WiFi credentials to NVS
fn save_wifi_credentials(nvs: &mut EspNvs<NvsDefault>, ssid: &str, password: &str) {
    if let Err(e) = nvs.set_str(KEY_WIFI_SSID, ssid) {
        error!("Failed to save SSID: {:?}", e);
    }
    if let Err(e) = nvs.set_str(KEY_WIFI_PASS, password) {
        error!("Failed to save password: {:?}", e);
    }
    info!("WiFi credentials saved to NVS");
}

/// Load WiFi credentials from NVS
pub fn load_wifi_credentials(nvs: &EspNvs<NvsDefault>) -> Option<(String, String)> {
    let mut ssid_buf = [0u8; 64];
    let mut pass_buf = [0u8; 128];

    let ssid = nvs.get_str(KEY_WIFI_SSID, &mut ssid_buf).ok()??;
    let pass = nvs.get_str(KEY_WIFI_PASS, &mut pass_buf).ok()??;

    if ssid.is_empty() {
        return None;
    }

    Some((ssid.to_string(), pass.to_string()))
}

/// Reset device - clear all NVS data
fn reset_device(nvs: &mut EspNvs<NvsDefault>) {
    // Clear WiFi credentials
    let _ = nvs.remove(KEY_WIFI_SSID);
    let _ = nvs.remove(KEY_WIFI_PASS);

    // Clear device state (keys, peers, invites)
    let _ = nvs.remove("secret_key");
    let _ = nvs.remove("state");
    let _ = nvs.remove("led_on");

    info!("Device reset complete - all settings cleared");
}
