use crate::error::{MakcuError, Result};
use crate::protocol::{builder, constants};
use crate::types::DeviceInfo;

use super::Device;

/// Strip the "km." prefix that the firmware prepends to some responses.
fn strip_km_prefix(s: &str) -> String {
    s.strip_prefix("km.").unwrap_or(s).to_string()
}

impl Device {
    /// Query the firmware version string (with "km." prefix stripped).
    pub fn version(&self) -> Result<String> {
        self.query(constants::CMD_VERSION).map(|v| strip_km_prefix(&v))
    }

    /// Returns combined device info (port name + firmware version).
    pub fn device_info(&self) -> Result<DeviceInfo> {
        let firmware = self.version()?;
        let port = self.port_name().to_string();
        Ok(DeviceInfo { port, firmware })
    }

    /// Query the current serial number reported by the connected mouse.
    pub fn serial(&self) -> Result<String> {
        self.query(constants::CMD_SERIAL_GET).map(|v| strip_km_prefix(&v))
    }

    /// Spoof the mouse serial number. Returns the device's response.
    ///
    /// The value must be at most 45 characters.
    pub fn set_serial(&self, value: &str) -> Result<String> {
        let cmd = builder::build_serial_set(value)
            .ok_or_else(|| MakcuError::Protocol("serial value too long".into()))?;
        self.query_dynamic(cmd.as_bytes()).map(|v| strip_km_prefix(&v))
    }

    /// Reset the spoofed serial back to the factory value.
    pub fn reset_serial(&self) -> Result<String> {
        self.query(constants::CMD_SERIAL_RESET).map(|v| strip_km_prefix(&v))
    }
}

// -- Async --

#[cfg(feature = "async")]
use super::AsyncDevice;

#[cfg(feature = "async")]
impl AsyncDevice {
    /// Query the firmware version string (with "km." prefix stripped).
    pub async fn version(&self) -> Result<String> {
        let v = self.query(constants::CMD_VERSION).await?;
        Ok(strip_km_prefix(&v))
    }

    /// Returns combined device info (port name + firmware version).
    pub async fn device_info(&self) -> Result<DeviceInfo> {
        let firmware = self.version().await?;
        let port = self.port_name().to_string();
        Ok(DeviceInfo { port, firmware })
    }

    pub async fn serial(&self) -> Result<String> {
        self.query(constants::CMD_SERIAL_GET).await.map(|v| strip_km_prefix(&v))
    }

    /// Spoof the mouse serial number. Returns the device's response.
    ///
    /// The value must be at most 45 characters.
    pub async fn set_serial(&self, value: &str) -> Result<String> {
        let cmd = builder::build_serial_set(value)
            .ok_or_else(|| MakcuError::Protocol("serial value too long".into()))?;
        self.query_dynamic(cmd.as_bytes()).await.map(|v| strip_km_prefix(&v))
    }

    pub async fn reset_serial(&self) -> Result<String> {
        self.query(constants::CMD_SERIAL_RESET).await.map(|v| strip_km_prefix(&v))
    }
}
