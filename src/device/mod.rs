mod buttons;
mod info;
mod locks;
mod movement;
mod stream;

use std::sync::mpsc;
use std::time::Duration;

use crate::error::{MakcuError, Result};
use crate::protocol::parser::{self, ResponseKind};
use crate::transport::TransportHandle;
use crate::transport::serial;
use crate::types::ConnectionState;

/// Default command timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(500);

/// Configuration for connecting to a MAKCU device.
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Serial port path. `None` = auto-detect by VID/PID.
    pub port: Option<String>,
    /// Try 4 Mbaud first before the baud-change sequence.
    pub try_4m_first: bool,
    /// Timeout for each command response.
    pub command_timeout: Duration,
    /// Enable automatic reconnection on disconnect.
    pub reconnect: bool,
    /// Initial reconnection backoff delay.
    pub reconnect_backoff: Duration,
    /// When true, all commands are fire-and-forget by default.
    pub fire_and_forget: bool,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            port: None,
            try_4m_first: true,
            command_timeout: DEFAULT_TIMEOUT,
            reconnect: true,
            reconnect_backoff: Duration::from_millis(100),
            fire_and_forget: false,
        }
    }
}

// ===========================================================================
// Device (sync)
// ===========================================================================

/// An open connection to a MAKCU device.
///
/// All methods take `&self` — the underlying I/O goes through channels.
/// `Device` is `Send + Sync` and can be wrapped in `Arc` for shared use.
pub struct Device {
    transport: TransportHandle,
    config: DeviceConfig,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("port", &self.transport.port_name())
            .field("connected", &self.transport.is_connected())
            .finish()
    }
}

// Compile-time assertions that Device is Send + Sync.
#[allow(dead_code)]
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn _assertions() {
        assert_send_sync::<Device>();
    }
};

impl Device {
    /// Find and connect to the first available MAKCU device.
    pub fn connect() -> Result<Self> {
        Self::with_config(DeviceConfig::default())
    }

    /// Connect to a specific port.
    pub fn connect_port(port: &str) -> Result<Self> {
        Self::with_config(DeviceConfig {
            port: Some(port.to_string()),
            ..Default::default()
        })
    }

    /// Connect with a custom configuration.
    pub fn with_config(config: DeviceConfig) -> Result<Self> {
        let port_name = match &config.port {
            Some(p) => p.clone(),
            None => serial::find_port()?,
        };

        let transport = TransportHandle::connect(
            port_name,
            config.try_4m_first,
            config.reconnect,
            config.reconnect_backoff,
        )?;

        Ok(Self { transport, config })
    }

    /// Disconnect from the device, shutting down all threads.
    pub fn disconnect(&self) {
        self.transport.shutdown();
    }

    /// Check if the device is currently connected.
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    /// Get the port name this device is connected to.
    pub fn port_name(&self) -> String {
        self.transport.port_name()
    }

    /// Subscribe to connection state changes.
    pub fn connection_events(&self) -> mpsc::Receiver<ConnectionState> {
        self.transport.subscribe_state()
    }

    /// Returns a fire-and-forget wrapper. Commands sent through this wrapper
    /// return immediately without waiting for the device response.
    pub fn ff(&self) -> FireAndForget<'_> {
        FireAndForget { device: self }
    }

    /// Send raw command bytes (escape hatch for unwrapped firmware commands).
    /// The `\r\n` terminator must already be included.
    pub fn send_raw(&self, cmd: &[u8]) -> Result<Vec<u8>> {
        self.transport
            .send_static(
                cmd,
                self.config.fire_and_forget,
                self.config.command_timeout,
            )?
            .ok_or(MakcuError::Protocol(
                "expected response but got fire-and-forget".into(),
            ))
    }

    /// Start building a batch of commands.
    #[cfg(feature = "batch")]
    pub fn batch(&self) -> crate::batch::BatchBuilder<'_> {
        crate::batch::BatchBuilder::new(self)
    }

    // -- Internal helpers --

    pub(crate) fn exec(&self, cmd: &[u8]) -> Result<()> {
        if self.config.fire_and_forget {
            self.transport
                .send_static(cmd, true, self.config.command_timeout)?;
            return Ok(());
        }
        let raw = self.send_raw(cmd)?;
        match parser::classify_response(&raw) {
            ResponseKind::Executed | ResponseKind::ValueOrEcho(_) | ResponseKind::Value(_) => {
                Ok(())
            }
        }
    }

    pub(crate) fn query(&self, cmd: &[u8]) -> Result<String> {
        let raw = self
            .transport
            .send_static(cmd, false, self.config.command_timeout)?
            .ok_or(MakcuError::Timeout)?;
        classify_as_value(&raw)
    }

    pub(crate) fn exec_dynamic(&self, cmd: &[u8]) -> Result<()> {
        self.transport.send_command(
            cmd.to_vec(),
            self.config.fire_and_forget,
            self.config.command_timeout,
        )?;
        Ok(())
    }

    pub(crate) fn query_dynamic(&self, cmd: &[u8]) -> Result<String> {
        let raw = self
            .transport
            .send_command(cmd.to_vec(), false, self.config.command_timeout)?
            .ok_or(MakcuError::Timeout)?;
        classify_as_value(&raw)
    }

    #[cfg(feature = "batch")]
    pub(crate) fn timeout(&self) -> Duration {
        self.config.command_timeout
    }

    pub(crate) fn transport(&self) -> &TransportHandle {
        &self.transport
    }
}

#[cfg(feature = "mock")]
impl Device {
    /// Create a Device backed by a mock transport (for testing).
    pub fn mock() -> (Self, std::sync::Arc<crate::transport::mock::MockTransport>) {
        let (transport, mock) = TransportHandle::from_mock();
        let device = Self {
            transport,
            config: DeviceConfig::default(),
        };
        (device, mock)
    }
}

// ===========================================================================
// FireAndForget (sync)
// ===========================================================================

/// Fire-and-forget wrapper. All commands return immediately after writing
/// to the transport channel, without waiting for the device response.
pub struct FireAndForget<'d> {
    device: &'d Device,
}

impl FireAndForget<'_> {
    pub(crate) fn send(&self, cmd: &[u8]) -> Result<()> {
        self.device
            .transport
            .send_static(cmd, true, self.device.config.command_timeout)?;
        Ok(())
    }

    pub(crate) fn send_dynamic(&self, cmd: &[u8]) -> Result<()> {
        self.device.transport.send_command(
            cmd.to_vec(),
            true,
            self.device.config.command_timeout,
        )?;
        Ok(())
    }
}

// ===========================================================================
// AsyncDevice
// ===========================================================================

#[cfg(feature = "async")]
pub struct AsyncDevice {
    transport: TransportHandle,
    config: DeviceConfig,
}

#[cfg(feature = "async")]
impl std::fmt::Debug for AsyncDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncDevice")
            .field("port", &self.transport.port_name())
            .field("connected", &self.transport.is_connected())
            .finish()
    }
}

#[cfg(feature = "async")]
impl AsyncDevice {
    /// Find and connect to the first available MAKCU device.
    pub async fn connect() -> Result<Self> {
        Self::with_config(DeviceConfig::default()).await
    }

    /// Connect to a specific port.
    pub async fn connect_port(port: &str) -> Result<Self> {
        Self::with_config(DeviceConfig {
            port: Some(port.to_string()),
            ..Default::default()
        })
        .await
    }

    /// Connect with a custom configuration.
    pub async fn with_config(config: DeviceConfig) -> Result<Self> {
        let cfg = config.clone();
        let (transport, config) = tokio::task::spawn_blocking(move || -> Result<_> {
            let port_name = match &cfg.port {
                Some(p) => p.clone(),
                None => serial::find_port()?,
            };
            let transport = TransportHandle::connect(
                port_name,
                cfg.try_4m_first,
                cfg.reconnect,
                cfg.reconnect_backoff,
            )?;
            Ok((transport, cfg))
        })
        .await
        .map_err(|e| MakcuError::Protocol(format!("join error: {}", e)))??;

        Ok(Self { transport, config })
    }

    /// Disconnect from the device, shutting down all threads.
    pub fn disconnect(&self) {
        self.transport.shutdown();
    }

    /// Check if the device is currently connected.
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    /// Get the port name this device is connected to.
    pub fn port_name(&self) -> String {
        self.transport.port_name()
    }

    /// Subscribe to connection state changes.
    pub fn connection_events(&self) -> mpsc::Receiver<ConnectionState> {
        self.transport.subscribe_state()
    }

    /// Returns a fire-and-forget wrapper.
    pub fn ff(&self) -> AsyncFireAndForget<'_> {
        AsyncFireAndForget { device: self }
    }

    /// Send raw command bytes (async escape hatch).
    pub async fn send_raw(&self, cmd: &[u8]) -> Result<Vec<u8>> {
        self.transport
            .send_static_async(
                cmd,
                self.config.fire_and_forget,
                self.config.command_timeout,
            )
            .await?
            .ok_or(MakcuError::Protocol(
                "expected response but got fire-and-forget".into(),
            ))
    }

    /// Start building a batch of commands.
    #[cfg(feature = "batch")]
    pub fn batch(&self) -> crate::batch::AsyncBatchBuilder<'_> {
        crate::batch::AsyncBatchBuilder::new(self)
    }

    // -- Internal async helpers --

    pub(crate) async fn exec(&self, cmd: &[u8]) -> Result<()> {
        if self.config.fire_and_forget {
            self.transport
                .send_static(cmd, true, self.config.command_timeout)?;
            return Ok(());
        }
        let raw = self.send_raw(cmd).await?;
        match parser::classify_response(&raw) {
            ResponseKind::Executed | ResponseKind::ValueOrEcho(_) | ResponseKind::Value(_) => {
                Ok(())
            }
        }
    }

    pub(crate) async fn query(&self, cmd: &[u8]) -> Result<String> {
        let raw = self
            .transport
            .send_static_async(cmd, false, self.config.command_timeout)
            .await?
            .ok_or(MakcuError::Timeout)?;
        classify_as_value(&raw)
    }

    pub(crate) async fn exec_dynamic(&self, cmd: &[u8]) -> Result<()> {
        self.transport
            .send_command_async(
                cmd.to_vec(),
                self.config.fire_and_forget,
                self.config.command_timeout,
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn query_dynamic(&self, cmd: &[u8]) -> Result<String> {
        let raw = self
            .transport
            .send_command_async(cmd.to_vec(), false, self.config.command_timeout)
            .await?
            .ok_or(MakcuError::Timeout)?;
        classify_as_value(&raw)
    }

    pub(crate) fn transport(&self) -> &TransportHandle {
        &self.transport
    }

    #[cfg(feature = "batch")]
    pub(crate) fn timeout(&self) -> std::time::Duration {
        self.config.command_timeout
    }
}

#[cfg(all(feature = "async", feature = "mock"))]
impl AsyncDevice {
    /// Create an AsyncDevice backed by a mock transport (for testing).
    pub fn mock() -> (Self, std::sync::Arc<crate::transport::mock::MockTransport>) {
        let (transport, mock) = TransportHandle::from_mock();
        let device = Self {
            transport,
            config: DeviceConfig::default(),
        };
        (device, mock)
    }
}

// ===========================================================================
// AsyncFireAndForget
// ===========================================================================

#[cfg(feature = "async")]
pub struct AsyncFireAndForget<'d> {
    device: &'d AsyncDevice,
}

#[cfg(feature = "async")]
impl AsyncFireAndForget<'_> {
    pub(crate) fn send(&self, cmd: &[u8]) -> Result<()> {
        self.device
            .transport
            .send_static(cmd, true, self.device.config.command_timeout)?;
        Ok(())
    }

    pub(crate) fn send_dynamic(&self, cmd: &[u8]) -> Result<()> {
        self.device.transport.send_command(
            cmd.to_vec(),
            true,
            self.device.config.command_timeout,
        )?;
        Ok(())
    }
}

// ===========================================================================
// Shared helpers
// ===========================================================================

fn classify_as_value(raw: &[u8]) -> Result<String> {
    match parser::classify_response(raw) {
        ResponseKind::Value(v) => Ok(v),
        ResponseKind::ValueOrEcho(v) => Ok(v),
        ResponseKind::Executed => Err(MakcuError::Protocol(
            "expected a value but got EXECUTED".into(),
        )),
    }
}
