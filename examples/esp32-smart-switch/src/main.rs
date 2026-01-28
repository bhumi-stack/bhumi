//! Bhumi Smart Switch for ESP32
//!
//! This firmware implements a smart electrical switch using the Bhumi P2P protocol.
//! It connects to a relay server via WiFi and responds to commands from paired devices.
//!
//! WiFi credentials are configured via BLE provisioning using the switch-controller CLI.

mod ble;
mod connection;
mod state;
mod switch;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::{Gpio2, Output, PinDriver},
        prelude::Peripherals,
    },
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::*;
use std::sync::{atomic::AtomicBool, Arc, Mutex};

const RELAY_ADDR: &str = "64.227.143.197:8443";

// Switch state
static IS_ON: AtomicBool = AtomicBool::new(false);

// LED control (GPIO2 is the built-in LED on most ESP32 dev boards)
static LED: Mutex<Option<PinDriver<'static, Gpio2, Output>>> = Mutex::new(None);

/// Set the LED state
pub fn set_led(on: bool) {
    if let Ok(mut guard) = LED.lock() {
        if let Some(led) = guard.as_mut() {
            if on {
                let _ = led.set_high();
            } else {
                let _ = led.set_low();
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!("Bhumi Smart Switch v0.2");
    info!("Initializing...");

    // Get peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Initialize LED on GPIO2
    let led = PinDriver::output(peripherals.pins.gpio2)?;
    *LED.lock().unwrap() = Some(led);
    info!("LED initialized on GPIO2");

    // Open NVS for BLE module
    let mut ble_nvs = EspNvs::new(nvs.clone(), "bhumi", true)?;

    // Initialize state from NVS
    let mut device_state = state::DeviceState::load(&nvs);
    info!("Device ID: {}", device_state.id52());

    // Start BLE server (always running for provisioning)
    let device_name = format!("Bhumi-Switch-{}", &device_state.id52()[..8]);
    let ble_state = ble::start_ble_server(&device_name);
    info!("BLE provisioning enabled");

    // Restore LED state from NVS
    let saved_led_state = device_state.load_led_state();
    IS_ON.store(saved_led_state, std::sync::atomic::Ordering::Relaxed);
    set_led(saved_led_state);
    info!(
        "Restored LED state: {}",
        if saved_led_state { "ON" } else { "OFF" }
    );

    // Get WiFi credentials from NVS (provisioned via BLE)
    let wifi_creds = ble::load_wifi_credentials(&ble_nvs);

    // Check if we have WiFi credentials
    if wifi_creds.is_none() {
        warn!("No WiFi credentials configured!");
        warn!("Use BLE provisioning (switch-controller ble provision) to configure WiFi");
        // Stay in BLE-only mode until credentials are provided
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if ble::check_ble_command(&ble_state, &mut ble_nvs) {
                info!("Restarting device...");
                restart_device();
            }
        }
    }

    let (wifi_ssid, wifi_pass) = wifi_creds.unwrap();
    info!("Using WiFi credentials from NVS: SSID='{}' (len={}), password len={}",
          wifi_ssid, wifi_ssid.len(), wifi_pass.len());

    // Connect to WiFi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs.clone()))?,
        sys_loop,
    )?;

    match connect_wifi(&mut wifi, &wifi_ssid, &wifi_pass) {
        Ok(()) => {
            info!(
                "WiFi connected, IP: {:?}",
                wifi.wifi().sta_netif().get_ip_info()?
            );
        }
        Err(e) => {
            error!("WiFi connection failed: {:?}", e);
            warn!("Waiting for BLE provisioning with new credentials...");
            // Stay in BLE-only mode until new credentials are provided
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if ble::check_ble_command(&ble_state, &mut ble_nvs) {
                    info!("Restarting device...");
                    restart_device();
                }
            }
        }
    }

    // Print invite token if no peers yet
    if device_state.peer_count() == 0 {
        let token = if device_state.invite_count() > 0 {
            device_state.get_invite_token().unwrap()
        } else {
            device_state.create_owner_invite()
        };
        info!("=== PAIRING MODE ===");
        info!("Share this invite token with the switch owner:");
        info!("  {}", token);
        info!("====================");
    } else {
        info!(
            "Paired: {} peer(s), {} pending invite(s)",
            device_state.peer_count(),
            device_state.invite_count()
        );
    }

    // Main loop - connect to relay and handle messages
    loop {
        // Check for BLE commands (reset, new credentials)
        if ble::check_ble_command(&ble_state, &mut ble_nvs) {
            info!("BLE command received, restarting device...");
            restart_device();
        }

        // Ensure WiFi is connected before attempting relay connection
        ensure_wifi_connected(&mut wifi, &ble_state, &mut ble_nvs);

        match run_connection(&mut device_state, &ble_state, &mut ble_nvs) {
            Ok(()) => info!("Connection closed, reconnecting..."),
            Err(e) => {
                error!("Connection error: {:?}", e);
                // Wait before reconnecting, but keep checking BLE
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if ble::check_ble_command(&ble_state, &mut ble_nvs) {
                        info!("BLE command received, restarting device...");
                        restart_device();
                    }
                }
            }
        }
    }
}

/// Restart the device
fn restart_device() -> ! {
    info!("Restarting in 1 second...");
    std::thread::sleep(std::time::Duration::from_secs(1));
    unsafe {
        esp_idf_svc::sys::esp_restart();
    }
}

/// Check WiFi connectivity and reconnect if needed
fn ensure_wifi_connected(
    wifi: &mut BlockingWifi<EspWifi<'static>>,
    ble_state: &Arc<Mutex<ble::BleState>>,
    ble_nvs: &mut EspNvs<NvsDefault>,
) {
    // Check if WiFi is connected
    if wifi.is_connected().unwrap_or(false) {
        return;
    }

    warn!("WiFi disconnected, attempting to reconnect...");

    // Try to reconnect with exponential backoff
    let mut retry_delay = 1;
    loop {
        // Check for BLE commands while reconnecting
        if ble::check_ble_command(ble_state, ble_nvs) {
            info!("BLE command received during reconnect, restarting...");
            restart_device();
        }

        // Try to reconnect
        match wifi.connect() {
            Ok(()) => {
                // Wait for IP
                match wifi.wait_netif_up() {
                    Ok(()) => {
                        if let Ok(ip_info) = wifi.wifi().sta_netif().get_ip_info() {
                            info!("WiFi reconnected, IP: {:?}", ip_info);
                        }
                        return;
                    }
                    Err(e) => {
                        warn!("Failed to get IP after reconnect: {:?}", e);
                    }
                }
            }
            Err(e) => {
                warn!(
                    "WiFi reconnect failed: {:?}, retrying in {}s...",
                    e, retry_delay
                );
            }
        }

        // Sleep in small increments to check BLE
        for _ in 0..(retry_delay * 10) {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if ble::check_ble_command(ble_state, ble_nvs) {
                info!("BLE command received during reconnect, restarting...");
                restart_device();
            }
        }
        retry_delay = (retry_delay * 2).min(60); // Cap at 60 seconds
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>, ssid: &str, password: &str) -> anyhow::Result<()> {
    use esp_idf_svc::wifi::AuthMethod;

    // Force WPA2 to avoid SAE negotiation issues with WPA2/WPA3 mixed networks
    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().map_err(|_| anyhow::anyhow!("SSID too long"))?,
        password: password.try_into().map_err(|_| anyhow::anyhow!("Password too long"))?,
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_config)?;
    wifi.start()?;

    // Scan for available networks (debug)
    info!("Scanning for WiFi networks...");
    let scan_result = wifi.scan()?;
    info!("Found {} networks:", scan_result.len());
    for ap in scan_result.iter().take(10) {
        info!(
            "  - {} (ch:{}, rssi:{}dBm, auth:{:?})",
            ap.ssid, ap.channel, ap.signal_strength, ap.auth_method
        );
    }

    info!("Connecting to {}...", ssid);

    wifi.connect()?;
    info!("WiFi connected!");

    wifi.wait_netif_up()?;
    info!("Network interface is up");

    Ok(())
}

fn run_connection(
    device_state: &mut state::DeviceState,
    ble_state: &Arc<Mutex<ble::BleState>>,
    ble_nvs: &mut EspNvs<NvsDefault>,
) -> anyhow::Result<()> {
    info!("Connecting to relay at {}...", RELAY_ADDR);

    let mut conn = connection::Connection::connect(
        RELAY_ADDR,
        device_state.secret_key(),
        device_state.get_commits(),
    )?;

    info!("Connected to relay");

    // Set a shorter read timeout so we can check BLE commands
    conn.set_read_timeout(Some(std::time::Duration::from_millis(500)))?;

    loop {
        // Check for BLE commands
        if ble::check_ble_command(ble_state, ble_nvs) {
            info!("BLE command received, restarting...");
            restart_device();
        }

        // Try to receive a message (with timeout)
        match conn.receive() {
            Ok(msg) => {
                // Check if it's a handshake or command
                if msg.is_handshake() {
                    if let Some((response, new_commit)) = device_state.handle_handshake(&msg) {
                        conn.send_ack(msg.msg_id, response)?;
                        if let Some(commit) = new_commit {
                            conn.update_commits(vec![commit])?;
                        }
                    } else {
                        conn.send_ack(msg.msg_id, device_state.reject_handshake())?;
                    }
                } else {
                    // Handle command
                    let (response, new_commit) = switch::handle_command(device_state, &msg);
                    let mut response_bytes = response;

                    // Append new preimage if available
                    if let Some((new_preimage, commit)) = new_commit {
                        response_bytes.extend_from_slice(&new_preimage);
                        conn.update_commits(vec![commit])?;
                    }

                    conn.send_ack(msg.msg_id, response_bytes)?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout - loop back and check BLE
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}
