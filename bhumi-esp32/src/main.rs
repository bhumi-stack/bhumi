//! Bhumi Smart Switch for ESP32
//!
//! This firmware implements a smart electrical switch using the Bhumi P2P protocol.
//! It connects to a relay server via WiFi and responds to commands from paired devices.

mod connection;
mod state;
mod switch;

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio::{Gpio2, Output, PinDriver},
        prelude::Peripherals,
    },
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::*;
use std::sync::{atomic::AtomicBool, Mutex};

// Configuration
// const WIFI_SSID: &str = "iPhone";
// const WIFI_PASS: &str = "dodododo";
const WIFI_SSID: &str = "A2102-One";
const WIFI_PASS: &str = "9820715512";
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

    info!("Bhumi Smart Switch v0.1");
    info!("Initializing...");

    // Get peripherals
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Initialize LED on GPIO2
    let led = PinDriver::output(peripherals.pins.gpio2)?;
    *LED.lock().unwrap() = Some(led);
    info!("LED initialized on GPIO2");

    // Initialize state from NVS
    let mut device_state = state::DeviceState::load(&nvs);
    info!("Device ID: {}", device_state.id52());

    // Restore LED state from NVS
    let saved_led_state = device_state.load_led_state();
    IS_ON.store(saved_led_state, std::sync::atomic::Ordering::Relaxed);
    set_led(saved_led_state);
    info!(
        "Restored LED state: {}",
        if saved_led_state { "ON" } else { "OFF" }
    );

    // Connect to WiFi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sys_loop.clone(), Some(nvs.clone()))?,
        sys_loop,
    )?;

    connect_wifi(&mut wifi)?;
    info!(
        "WiFi connected, IP: {:?}",
        wifi.wifi().sta_netif().get_ip_info()?
    );

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
        // Ensure WiFi is connected before attempting relay connection
        ensure_wifi_connected(&mut wifi);

        match run_connection(&mut device_state) {
            Ok(()) => info!("Connection closed, reconnecting..."),
            Err(e) => {
                error!("Connection error: {:?}", e);
                // Wait before reconnecting
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
}

/// Check WiFi connectivity and reconnect if needed
fn ensure_wifi_connected(wifi: &mut BlockingWifi<EspWifi<'static>>) {
    // Check if WiFi is connected
    if wifi.is_connected().unwrap_or(false) {
        return;
    }

    warn!("WiFi disconnected, attempting to reconnect...");

    // Try to reconnect with exponential backoff
    let mut retry_delay = 1;
    loop {
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

        std::thread::sleep(std::time::Duration::from_secs(retry_delay));
        retry_delay = (retry_delay * 2).min(60); // Cap at 60 seconds
    }
}

fn connect_wifi(wifi: &mut BlockingWifi<EspWifi<'static>>) -> anyhow::Result<()> {
    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().unwrap(),
        password: WIFI_PASS.try_into().unwrap(),
        ..Default::default()
    });

    wifi.set_configuration(&wifi_config)?;
    wifi.start()?;
    info!("WiFi started, connecting to {}...", WIFI_SSID);

    wifi.connect()?;
    info!("WiFi connected!");

    wifi.wait_netif_up()?;
    info!("Network interface is up");

    Ok(())
}

fn run_connection(device_state: &mut state::DeviceState) -> anyhow::Result<()> {
    info!("Connecting to relay at {}...", RELAY_ADDR);

    let mut conn = connection::Connection::connect(
        RELAY_ADDR,
        device_state.secret_key(),
        device_state.get_commits(),
    )?;

    info!("Connected to relay");

    loop {
        let msg = conn.receive()?;

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
}
