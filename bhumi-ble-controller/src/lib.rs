//! Bhumi BLE Controller
//!
//! BLE client for provisioning and managing Bhumi IoT devices.
//!
//! # Example
//!
//! ```ignore
//! use bhumi_ble_controller::ble;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Scan for devices
//!     let devices = ble::scan(5).await?;
//!     for device in &devices {
//!         println!("{} ({})", device.name, device.address);
//!     }
//!
//!     // Provision a device
//!     ble::provision(None, "MySSID", "MyPassword").await?;
//!
//!     // Reset a device
//!     ble::reset(None).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod ble;
