//! Bhumi MCU Library
//!
//! Traits and protocols for building Bhumi IoT devices on microcontrollers.
//!
//! This crate provides:
//! - BLE GATT service protocol for WiFi provisioning
//! - Traits for WiFi, BLE, and persistent storage
//!
//! # Example MCU implementations
//! - ESP32: See `bhumi-esp32` example
//! - Pico W: See `bhumi-pico` example (coming soon)
//!
//! # Note
//! This crate has no dependencies. MCU implementations use `bhumi-proto`
//! directly for protocol types (sync) or `bhumi-node` (async).

pub mod ble;
pub mod storage;
pub mod wifi;

pub use ble::*;
pub use storage::*;
pub use wifi::*;
