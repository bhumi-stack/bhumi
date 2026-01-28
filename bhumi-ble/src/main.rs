//! BLE provisioning tool for Bhumi devices
//!
//! Scans for Bhumi devices and sends WiFi credentials via BLE.

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use clap::{Parser, Subcommand};
use std::time::Duration;
use uuid::Uuid;

// Bhumi BLE Service UUID (custom 128-bit UUID)
// Format: b40e1000-5e7c-1c3e-0000-000000000000
#[allow(dead_code)]
const BHUMI_SERVICE_UUID: Uuid = Uuid::from_u128(0xb40e1000_5e7c_1c3e_0000_000000000000);

// Characteristic UUIDs
const WIFI_SSID_UUID: Uuid = Uuid::from_u128(0xb40e1001_5e7c_1c3e_0000_000000000000);
const WIFI_PASS_UUID: Uuid = Uuid::from_u128(0xb40e1002_5e7c_1c3e_0000_000000000000);
const COMMAND_UUID: Uuid = Uuid::from_u128(0xb40e1003_5e7c_1c3e_0000_000000000000);
#[allow(dead_code)]
const STATUS_UUID: Uuid = Uuid::from_u128(0xb40e1004_5e7c_1c3e_0000_000000000000);

// Commands
const CMD_RESET: u8 = 0x01;
const CMD_PROVISION: u8 = 0x02;

#[derive(Parser)]
#[command(name = "bhumi-ble")]
#[command(about = "BLE provisioning tool for Bhumi devices")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for Bhumi devices
    Scan {
        /// Scan duration in seconds
        #[arg(short, long, default_value = "5")]
        duration: u64,
    },
    /// Send WiFi credentials to a device
    Provision {
        /// Device name or address to connect to
        #[arg(short, long)]
        device: Option<String>,
        /// WiFi credentials file (SSID on line 1, password on line 2)
        #[arg(short, long, default_value = "wifi_credentials.txt")]
        file: String,
    },
    /// Reset a device (clear all settings)
    Reset {
        /// Device name or address to connect to
        #[arg(short, long)]
        device: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters.into_iter().next().ok_or("No Bluetooth adapter found")?;

    match cli.command {
        Commands::Scan { duration } => {
            scan_devices(&adapter, duration).await?;
        }
        Commands::Provision { device, file } => {
            let (ssid, password) = read_wifi_credentials(&file)?;
            provision_device(&adapter, device, &ssid, &password).await?;
        }
        Commands::Reset { device } => {
            reset_device(&adapter, device).await?;
        }
    }

    Ok(())
}

fn read_wifi_credentials(file: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(file)?;
    let mut lines = content.lines();
    let ssid = lines.next().ok_or("Missing SSID in credentials file")?.trim().to_string();
    let password = lines.next().ok_or("Missing password in credentials file")?.trim().to_string();
    Ok((ssid, password))
}

async fn scan_devices(adapter: &Adapter, duration: u64) -> Result<(), Box<dyn std::error::Error>> {
    println!("Scanning for Bhumi devices ({} seconds)...", duration);

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(duration)).await;

    let peripherals = adapter.peripherals().await?;

    println!("\nFound {} devices:", peripherals.len());
    for peripheral in peripherals {
        let props = peripheral.properties().await?;
        if let Some(props) = props {
            let name = props.local_name.unwrap_or_else(|| "Unknown".to_string());
            let addr = peripheral.address();
            let rssi = props.rssi.map(|r| format!("{} dBm", r)).unwrap_or_else(|| "N/A".to_string());

            // Check if this is a Bhumi device (name starts with "Bhumi")
            let is_bhumi = name.starts_with("Bhumi");
            let marker = if is_bhumi { " [BHUMI]" } else { "" };

            println!("  {} ({}) RSSI: {}{}", name, addr, rssi, marker);
        }
    }

    adapter.stop_scan().await?;
    Ok(())
}

async fn find_bhumi_device(
    adapter: &Adapter,
    target: Option<String>,
) -> Result<Peripheral, Box<dyn std::error::Error>> {
    println!("Scanning for Bhumi devices...");

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let peripherals = adapter.peripherals().await?;

    for peripheral in peripherals {
        let props = peripheral.properties().await?;
        if let Some(props) = props {
            let name = props.local_name.unwrap_or_default();
            let addr = peripheral.address().to_string();

            // Match by target (name or address) or find any Bhumi device
            let matches = match &target {
                Some(t) => name.contains(t) || addr.contains(t),
                None => name.starts_with("Bhumi"),
            };

            if matches {
                adapter.stop_scan().await?;
                println!("Found device: {} ({})", name, addr);
                return Ok(peripheral);
            }
        }
    }

    adapter.stop_scan().await?;
    Err("No Bhumi device found".into())
}

async fn provision_device(
    adapter: &Adapter,
    target: Option<String>,
    ssid: &str,
    password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = find_bhumi_device(adapter, target).await?;

    println!("Connecting...");
    device.connect().await?;
    println!("Connected!");

    println!("Discovering services...");
    device.discover_services().await?;

    let characteristics = device.characteristics();

    // Find WiFi SSID characteristic
    let ssid_char = characteristics.iter()
        .find(|c| c.uuid == WIFI_SSID_UUID)
        .ok_or("WiFi SSID characteristic not found")?;

    // Find WiFi password characteristic
    let pass_char = characteristics.iter()
        .find(|c| c.uuid == WIFI_PASS_UUID)
        .ok_or("WiFi password characteristic not found")?;

    // Find command characteristic
    let cmd_char = characteristics.iter()
        .find(|c| c.uuid == COMMAND_UUID)
        .ok_or("Command characteristic not found")?;

    println!("Sending WiFi credentials...");
    println!("  SSID: {}", ssid);

    device.write(ssid_char, ssid.as_bytes(), WriteType::WithResponse).await?;
    device.write(pass_char, password.as_bytes(), WriteType::WithResponse).await?;
    device.write(cmd_char, &[CMD_PROVISION], WriteType::WithResponse).await?;

    println!("WiFi credentials sent! Device will restart and connect to WiFi.");

    device.disconnect().await?;
    Ok(())
}

async fn reset_device(
    adapter: &Adapter,
    target: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let device = find_bhumi_device(adapter, target).await?;

    println!("Connecting...");
    device.connect().await?;
    println!("Connected!");

    println!("Discovering services...");
    device.discover_services().await?;

    let characteristics = device.characteristics();

    // Find command characteristic
    let cmd_char = characteristics.iter()
        .find(|c| c.uuid == COMMAND_UUID)
        .ok_or("Command characteristic not found")?;

    println!("Sending reset command...");
    device.write(cmd_char, &[CMD_RESET], WriteType::WithResponse).await?;

    println!("Reset command sent! Device will clear all settings and restart.");

    // Device will disconnect automatically after reset
    let _ = device.disconnect().await;
    Ok(())
}
