//! BLE Client for provisioning Bhumi devices
//!
//! Provides functions to scan, provision, and reset Bhumi IoT devices via BLE.

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use std::time::Duration;
use uuid::Uuid;

use bhumi_proto::ble::{WIFI_SSID_UUID, WIFI_PASS_UUID, COMMAND_UUID, commands};

/// A discovered Bhumi device
#[derive(Debug, Clone)]
pub struct BhumiDevice {
    pub name: String,
    pub address: String,
    pub rssi: Option<i16>,
    pub is_bhumi: bool,
}

/// Parse UUID string into uuid::Uuid
fn parse_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).expect("invalid UUID in bhumi_proto")
}

/// Get the default Bluetooth adapter
pub async fn get_adapter() -> Result<Adapter, Box<dyn std::error::Error>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    adapters.into_iter().next().ok_or_else(|| "No Bluetooth adapter found".into())
}

/// Scan for BLE devices
///
/// Returns a list of discovered devices. Bhumi devices have `is_bhumi = true`.
pub async fn scan(duration_secs: u64) -> Result<Vec<BhumiDevice>, Box<dyn std::error::Error>> {
    let adapter = get_adapter().await?;

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(duration_secs)).await;

    let peripherals = adapter.peripherals().await?;
    let mut devices = Vec::new();

    for peripheral in peripherals {
        if let Some(props) = peripheral.properties().await? {
            let name = props.local_name.unwrap_or_else(|| "Unknown".to_string());
            let address = peripheral.address().to_string();
            let rssi = props.rssi;
            // Match "Bhumi-xxx" or "nimble [Bhumi-xxx]" format
            let is_bhumi = name.starts_with("Bhumi") || name.contains("[Bhumi");

            devices.push(BhumiDevice { name, address, rssi, is_bhumi });
        }
    }

    adapter.stop_scan().await?;
    Ok(devices)
}

/// Find a Bhumi device by name/address pattern, or find any Bhumi device
pub async fn find_device(target: Option<&str>) -> Result<Peripheral, Box<dyn std::error::Error>> {
    let adapter = get_adapter().await?;

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let peripherals = adapter.peripherals().await?;

    for peripheral in peripherals {
        if let Some(props) = peripheral.properties().await? {
            let name = props.local_name.unwrap_or_default();
            let addr = peripheral.address().to_string();

            let matches = match target {
                Some(t) => name.contains(t) || addr.contains(t),
                // Match "Bhumi-xxx" or "nimble [Bhumi-xxx]" format
                None => name.starts_with("Bhumi") || name.contains("[Bhumi"),
            };

            if matches {
                adapter.stop_scan().await?;
                return Ok(peripheral);
            }
        }
    }

    adapter.stop_scan().await?;
    Err("No Bhumi device found".into())
}

/// Provision a device with WiFi credentials
///
/// # Arguments
/// * `target` - Device name/address pattern, or None to find any Bhumi device
/// * `ssid` - WiFi network name
/// * `password` - WiFi password
pub async fn provision(
    target: Option<&str>,
    ssid: &str,
    password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = find_device(target).await?;

    device.connect().await?;
    device.discover_services().await?;

    let characteristics = device.characteristics();

    let ssid_uuid = parse_uuid(WIFI_SSID_UUID);
    let pass_uuid = parse_uuid(WIFI_PASS_UUID);
    let cmd_uuid = parse_uuid(COMMAND_UUID);

    let ssid_char = characteristics.iter()
        .find(|c| c.uuid == ssid_uuid)
        .ok_or("WiFi SSID characteristic not found")?;

    let pass_char = characteristics.iter()
        .find(|c| c.uuid == pass_uuid)
        .ok_or("WiFi password characteristic not found")?;

    let cmd_char = characteristics.iter()
        .find(|c| c.uuid == cmd_uuid)
        .ok_or("Command characteristic not found")?;

    device.write(ssid_char, ssid.as_bytes(), WriteType::WithResponse).await?;
    device.write(pass_char, password.as_bytes(), WriteType::WithResponse).await?;
    device.write(cmd_char, &[commands::PROVISION], WriteType::WithResponse).await?;

    device.disconnect().await?;
    Ok(())
}

/// Reset a device (clear all settings)
///
/// # Arguments
/// * `target` - Device name/address pattern, or None to find any Bhumi device
pub async fn reset(target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let device = find_device(target).await?;

    device.connect().await?;
    device.discover_services().await?;

    let characteristics = device.characteristics();
    let cmd_uuid = parse_uuid(COMMAND_UUID);

    let cmd_char = characteristics.iter()
        .find(|c| c.uuid == cmd_uuid)
        .ok_or("Command characteristic not found")?;

    device.write(cmd_char, &[commands::RESET], WriteType::WithResponse).await?;

    let _ = device.disconnect().await;
    Ok(())
}
