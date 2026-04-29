use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use rustmod_client::{
    ClientConfig, ClientError, ModbusClient, ReadDeviceIdentificationResponse, UnitId,
};
use rustmod_core::pdu::exception::ExceptionCode;
use rustmod_datalink::{ModbusRtuConfig, ModbusRtuTransport, ModbusTcpTransport};
use tokio::task::JoinHandle;

use crate::config::loader::LoadedDevice;
use crate::config::profile::{
    ByteOrder, ModbusDataType, ModbusPointMapping, ModbusRegisterType, PointAccess, PointValue,
};
use crate::config::scenario::{ModbusNetworkConfig, ScenarioSettings};
use crate::event::bus::{Event, EventBus};
use crate::store::point_store::{PointKey, PointStatusFlags, PointStore};

use super::backoff::DeviceBackoff;
use super::traits::BridgeError;

// ---------------------------------------------------------------------------
// Structured Modbus error type
// ---------------------------------------------------------------------------

/// Structured Modbus error preserving exception codes from the device.
#[derive(Debug)]
pub enum ModbusError {
    /// Device returned a Modbus exception response.
    Exception {
        function_code: u8,
        exception: ExceptionCode,
    },
    /// Transport-level I/O error (connection dropped, etc).
    Transport(String),
    /// Request timed out.
    Timeout,
    /// Response was structurally invalid.
    InvalidResponse(String),
    /// Encoding error.
    Encode(String),
}

impl fmt::Display for ModbusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exception {
                function_code,
                exception,
            } => {
                let desc = match exception {
                    ExceptionCode::IllegalFunction => "Illegal Function",
                    ExceptionCode::IllegalDataAddress => "Illegal Data Address",
                    ExceptionCode::IllegalDataValue => "Illegal Data Value",
                    ExceptionCode::ServerDeviceFailure => "Server Device Failure",
                    ExceptionCode::Acknowledge => "Acknowledge",
                    ExceptionCode::ServerDeviceBusy => "Server Device Busy",
                    ExceptionCode::MemoryParityError => "Memory Parity Error",
                    ExceptionCode::GatewayPathUnavailable => "Gateway Path Unavailable",
                    ExceptionCode::GatewayTargetFailedToRespond => "Gateway Target Failed",
                    ExceptionCode::Unknown(code) => {
                        return write!(f, "Exception 0x{code:02X} (FC{function_code})")
                    }
                    _ => return write!(f, "Exception (FC{function_code})"),
                };
                write!(f, "{desc} (FC{function_code})")
            }
            Self::Transport(msg) => write!(f, "Transport: {msg}"),
            Self::Timeout => write!(f, "Request timed out"),
            Self::InvalidResponse(msg) => write!(f, "Invalid response: {msg}"),
            Self::Encode(msg) => write!(f, "Encode: {msg}"),
        }
    }
}

impl From<ClientError> for ModbusError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Exception(ex) => ModbusError::Exception {
                function_code: ex.function_code,
                exception: ex.exception_code,
            },
            ClientError::Timeout => ModbusError::Timeout,
            ClientError::DataLink(dl) => ModbusError::Transport(dl.to_string()),
            ClientError::InvalidResponse(kind) => ModbusError::InvalidResponse(format!("{kind}")),
            ClientError::Encode(enc) => ModbusError::Encode(enc.to_string()),
            ClientError::Decode(dec) => ModbusError::InvalidResponse(dec.to_string()),
            _ => ModbusError::Transport(format!("{e}")),
        }
    }
}

impl ModbusError {
    /// Whether this error suggests the device is temporarily busy and we should retry.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Exception {
                exception: ExceptionCode::ServerDeviceBusy,
                ..
            }
        )
    }

    /// Whether this is a transport-level error suggesting reconnection may help.
    pub fn is_transport(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::Timeout)
    }
}

// ---------------------------------------------------------------------------
// Transport abstraction
// ---------------------------------------------------------------------------

pub enum ModbusTransport {
    Tcp(ModbusClient<ModbusTcpTransport>),
    Rtu(ModbusClient<ModbusRtuTransport>),
}

/// Macro to dispatch a method call to the inner client, converting ClientError → ModbusError.
macro_rules! dispatch {
    ($self:expr, $method:ident, $($arg:expr),*) => {
        match $self {
            ModbusTransport::Tcp(c) => c.$method($($arg),*).await.map_err(ModbusError::from),
            ModbusTransport::Rtu(c) => c.$method($($arg),*).await.map_err(ModbusError::from),
        }
    };
}

impl ModbusTransport {
    async fn read_coils(
        &self,
        unit_id: UnitId,
        start: u16,
        qty: u16,
    ) -> Result<Vec<bool>, ModbusError> {
        dispatch!(self, read_coils, unit_id, start, qty)
    }

    async fn read_discrete_inputs(
        &self,
        unit_id: UnitId,
        start: u16,
        qty: u16,
    ) -> Result<Vec<bool>, ModbusError> {
        dispatch!(self, read_discrete_inputs, unit_id, start, qty)
    }

    async fn read_holding_registers(
        &self,
        unit_id: UnitId,
        start: u16,
        qty: u16,
    ) -> Result<Vec<u16>, ModbusError> {
        dispatch!(self, read_holding_registers, unit_id, start, qty)
    }

    async fn read_input_registers(
        &self,
        unit_id: UnitId,
        start: u16,
        qty: u16,
    ) -> Result<Vec<u16>, ModbusError> {
        dispatch!(self, read_input_registers, unit_id, start, qty)
    }

    async fn write_single_coil(
        &self,
        unit_id: UnitId,
        addr: u16,
        val: bool,
    ) -> Result<(), ModbusError> {
        dispatch!(self, write_single_coil, unit_id, addr, val)
    }

    async fn write_single_register(
        &self,
        unit_id: UnitId,
        addr: u16,
        val: u16,
    ) -> Result<(), ModbusError> {
        dispatch!(self, write_single_register, unit_id, addr, val)
    }

    async fn write_multiple_registers(
        &self,
        unit_id: UnitId,
        addr: u16,
        vals: &[u16],
    ) -> Result<(), ModbusError> {
        dispatch!(self, write_multiple_registers, unit_id, addr, vals)
    }

    async fn mask_write_register(
        &self,
        unit_id: UnitId,
        addr: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), ModbusError> {
        dispatch!(self, mask_write_register, unit_id, addr, and_mask, or_mask)
    }

    async fn diagnostics(
        &self,
        unit_id: UnitId,
        sub_function: u16,
        data: u16,
    ) -> Result<(u16, u16), ModbusError> {
        dispatch!(self, diagnostics, unit_id, sub_function, data)
    }

    async fn read_fifo_queue(&self, unit_id: UnitId, addr: u16) -> Result<Vec<u16>, ModbusError> {
        dispatch!(self, read_fifo_queue, unit_id, addr)
    }

    async fn read_write_multiple_registers(
        &self,
        unit_id: UnitId,
        read_start: u16,
        read_qty: u16,
        write_start: u16,
        write_values: &[u16],
    ) -> Result<Vec<u16>, ModbusError> {
        dispatch!(
            self,
            read_write_multiple_registers,
            unit_id,
            read_start,
            read_qty,
            write_start,
            write_values
        )
    }

    async fn read_device_identification(
        &self,
        unit_id: UnitId,
        code: u8,
        object_id: u8,
    ) -> Result<ReadDeviceIdentificationResponse, ModbusError> {
        dispatch!(self, read_device_identification, unit_id, code, object_id)
    }
}

impl Clone for ModbusTransport {
    fn clone(&self) -> Self {
        match self {
            Self::Tcp(c) => Self::Tcp(c.clone()),
            Self::Rtu(c) => Self::Rtu(c.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Public info types for discovery
// ---------------------------------------------------------------------------

/// Public view of a configured Modbus device for discovery.
#[derive(Debug, Clone)]
pub struct ModbusDeviceInfo {
    pub instance_id: String,
    pub host: String,
    pub port: u16,
    pub unit_id: u8,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub firmware_revision: Option<String>,
    pub points: Vec<ModbusPointInfo>,
}

#[derive(Debug, Clone)]
pub struct ModbusPointInfo {
    pub point_id: String,
    pub writable: bool,
    pub register_type: ModbusRegisterType,
    pub address: u16,
    pub data_type: Option<ModbusDataType>,
    pub scale: Option<f64>,
}

/// Device identification info from FC43.
#[derive(Debug, Clone, Default)]
pub struct DeviceIdInfo {
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub revision: Option<String>,
}

// ---------------------------------------------------------------------------
// Config types built from loaded scenario
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ModbusDeviceConfig {
    instance_id: String,
    host: String,
    port: u16,
    unit_id: u8,
    byte_order: ByteOrder,
    word_order: ByteOrder,
    response_timeout_ms: Option<u64>,
    retry_count: Option<u8>,
    throttle_delay_ms: Option<u64>,
    points: Vec<ModbusPointConfig>,
}

#[derive(Debug, Clone)]
struct ModbusPointConfig {
    point_id: String,
    access: PointAccess,
    mapping: ModbusPointMapping,
}

// ---------------------------------------------------------------------------
// Block read planning (Phase 2)
// ---------------------------------------------------------------------------

/// Maximum number of holding/input registers per block read (Modbus limit: 125).
const MAX_REGISTER_BLOCK: u16 = 125;
/// Maximum number of coils/discrete inputs per block read (Modbus limit: 2000).
const MAX_BIT_BLOCK: u16 = 2000;
/// Maximum gap between registers to merge into a single block.
const MAX_BLOCK_GAP: u16 = 10;

#[derive(Debug)]
struct RegisterBlock {
    register_type: ModbusRegisterType,
    start_address: u16,
    count: u16,
    points: Vec<BlockPointRef>,
}

#[derive(Debug)]
struct BlockPointRef {
    point_index: usize,
    offset: u16,
    reg_count: u16,
}

/// Group points by (register_type, unit_id) and merge contiguous/nearby into blocks.
fn plan_block_reads(points: &[ModbusPointConfig]) -> Vec<RegisterBlock> {
    // Separate by register type
    let mut by_type: HashMap<&str, Vec<(usize, &ModbusPointConfig)>> = HashMap::new();
    for (i, pt) in points.iter().enumerate() {
        let key = match pt.mapping.register_type {
            ModbusRegisterType::Holding => "holding",
            ModbusRegisterType::Input => "input",
            ModbusRegisterType::Coil => "coil",
            ModbusRegisterType::DiscreteInput => "discrete",
        };
        by_type.entry(key).or_default().push((i, pt));
    }

    let mut blocks = Vec::new();

    for (type_key, mut pts) in by_type {
        let reg_type = match type_key {
            "holding" => ModbusRegisterType::Holding,
            "input" => ModbusRegisterType::Input,
            "coil" => ModbusRegisterType::Coil,
            _ => ModbusRegisterType::DiscreteInput,
        };

        let is_bit = matches!(
            reg_type,
            ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput
        );
        let max_block = if is_bit {
            MAX_BIT_BLOCK
        } else {
            MAX_REGISTER_BLOCK
        };

        // Sort by address
        pts.sort_by_key(|(_, pt)| pt.mapping.address);

        let mut current_block: Option<RegisterBlock> = None;

        for (idx, pt) in pts {
            let addr = pt.mapping.address;
            let dt = pt
                .mapping
                .data_type
                .clone()
                .unwrap_or(default_data_type(&reg_type));
            let rc = if is_bit {
                1
            } else {
                register_count(&dt, pt.mapping.register_count)
            };

            if let Some(ref mut blk) = current_block {
                let end_of_block = blk.start_address + blk.count;
                let gap = addr.saturating_sub(end_of_block);
                let new_count = (addr + rc).saturating_sub(blk.start_address);

                if gap <= MAX_BLOCK_GAP && new_count <= max_block {
                    // Extend current block
                    blk.count = new_count;
                    blk.points.push(BlockPointRef {
                        point_index: idx,
                        offset: addr - blk.start_address,
                        reg_count: rc,
                    });
                    continue;
                }

                // Finalize current block and start a new one
                blocks.push(current_block.take().unwrap());
            }

            current_block = Some(RegisterBlock {
                register_type: reg_type.clone(),
                start_address: addr,
                count: rc,
                points: vec![BlockPointRef {
                    point_index: idx,
                    offset: 0,
                    reg_count: rc,
                }],
            });
        }

        if let Some(blk) = current_block {
            blocks.push(blk);
        }
    }

    blocks
}

// ---------------------------------------------------------------------------
// ModbusConfig from scenario
// ---------------------------------------------------------------------------

pub fn modbus_config_from_scenario(
    settings: &Option<ScenarioSettings>,
) -> Option<ModbusNetworkConfig> {
    settings.as_ref()?.modbus.clone()
}

// ---------------------------------------------------------------------------
// ModbusBridge — client-side Modbus TCP/RTU integration
// ---------------------------------------------------------------------------

pub struct ModbusBridge {
    poll_interval: Duration,
    devices: Vec<ModbusDeviceConfig>,
    clients: HashMap<String, Arc<ModbusTransport>>,
    poll_handles: Vec<JoinHandle<()>>,
    store: Option<PointStore>,
    event_bus: Option<EventBus>,
    network_config: Option<ModbusNetworkConfig>,
}

impl Default for ModbusBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ModbusBridge {
    pub fn new() -> Self {
        ModbusBridge {
            poll_interval: Duration::from_secs(10),
            devices: Vec::new(),
            clients: HashMap::new(),
            poll_handles: Vec::new(),
            store: None,
            event_bus: None,
            network_config: None,
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn with_modbus_config(mut self, config: Option<ModbusNetworkConfig>) -> Self {
        self.network_config = config;
        self
    }

    /// Build device configs from loaded scenario devices.
    /// Only includes devices that have Modbus mappings AND a configured host (TCP) or serial port (RTU).
    pub fn from_loaded_devices(mut self, devices: &[LoadedDevice]) -> Self {
        let is_rtu = self
            .network_config
            .as_ref()
            .and_then(|c| c.mode.as_deref())
            .map(|m| m == "rtu")
            .unwrap_or(false);

        self.devices = devices
            .iter()
            .filter_map(|dev| {
                let defaults = dev
                    .profile
                    .defaults
                    .as_ref()?
                    .protocols
                    .as_ref()?
                    .modbus
                    .as_ref()?;

                // RTU mode doesn't require host — serial port is in network config
                if !is_rtu {
                    defaults.host.as_ref()?;
                }

                let points: Vec<ModbusPointConfig> = dev
                    .profile
                    .points
                    .iter()
                    .filter_map(|pt| {
                        let mapping = pt.protocols.as_ref()?.modbus.clone()?;
                        Some(ModbusPointConfig {
                            point_id: pt.id.clone(),
                            access: pt.access.clone(),
                            mapping,
                        })
                    })
                    .collect();

                if points.is_empty() {
                    return None;
                }

                Some(ModbusDeviceConfig {
                    instance_id: dev.instance_id.clone(),
                    host: defaults.host.clone().unwrap_or_default(),
                    port: defaults.port.unwrap_or(502),
                    unit_id: defaults.unit_id.unwrap_or(1),
                    byte_order: defaults.byte_order.clone().unwrap_or(ByteOrder::BigEndian),
                    word_order: defaults.word_order.clone().unwrap_or(ByteOrder::BigEndian),
                    response_timeout_ms: defaults.response_timeout_ms,
                    retry_count: defaults.retry_count,
                    throttle_delay_ms: defaults.throttle_delay_ms,
                    points,
                })
            })
            .collect();

        self
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn point_count(&self) -> usize {
        self.devices.iter().map(|d| d.points.len()).sum()
    }

    /// Whether the bridge is configured in RTU (serial) mode.
    pub fn is_rtu(&self) -> bool {
        self.network_config
            .as_ref()
            .and_then(|c| c.mode.as_deref())
            .map(|m| m == "rtu")
            .unwrap_or(false)
    }

    /// Return a public snapshot of all configured Modbus devices for discovery.
    pub fn discovered_devices(&self) -> Vec<ModbusDeviceInfo> {
        self.devices
            .iter()
            .map(|dev| ModbusDeviceInfo {
                instance_id: dev.instance_id.clone(),
                host: dev.host.clone(),
                port: dev.port,
                unit_id: dev.unit_id,
                vendor: None,
                model: None,
                firmware_revision: None,
                points: dev
                    .points
                    .iter()
                    .map(|pt| ModbusPointInfo {
                        point_id: pt.point_id.clone(),
                        writable: !matches!(pt.access, PointAccess::Input),
                        register_type: pt.mapping.register_type.clone(),
                        address: pt.mapping.address,
                        data_type: pt.mapping.data_type.clone(),
                        scale: pt.mapping.scale,
                    })
                    .collect(),
            })
            .collect()
    }

    /// Enrich a device info with FC43 device identification (vendor/product/revision).
    /// Swallows errors — many devices don't support FC43.
    pub async fn enrich_device_id(&self, info: &mut ModbusDeviceInfo) {
        if let Some(client) = self.clients.get(&info.instance_id) {
            let unit_id = UnitId::new(info.unit_id);
            if let Ok(resp) = client.read_device_identification(unit_id, 0x01, 0x00).await {
                for obj in &resp.objects {
                    let val = String::from_utf8_lossy(&obj.value).to_string();
                    match obj.object_id {
                        0x00 => info.vendor = Some(val),
                        0x01 => info.model = Some(val),
                        0x02 => info.firmware_revision = Some(val),
                        _ => {}
                    }
                }
            }
        }
    }

    /// Check if a configured device is reachable by attempting a test read.
    /// Tries holding registers, input registers, and coils (some devices only support certain FCs).
    pub async fn check_device_online(&self, instance_id: &str, unit_id: u8) -> bool {
        let client = match self.clients.get(instance_id) {
            Some(c) => c,
            None => return false,
        };
        let uid = UnitId::new(unit_id);
        client.read_holding_registers(uid, 0, 1).await.is_ok()
            || client.read_input_registers(uid, 0, 1).await.is_ok()
            || client.read_coils(uid, 0, 1).await.is_ok()
    }

    /// Scan a TCP host (or the RTU bus) for responding Modbus devices by probing unit IDs.
    /// For each responding unit ID, tries FC43 device identification.
    /// Returns discovered devices (no points — those come from config or manual browsing).
    pub async fn scan_unit_ids(
        &self,
        host: &str,
        port: u16,
        start_unit: u8,
        end_unit: u8,
    ) -> Vec<ModbusDeviceInfo> {
        let mut results = Vec::new();

        // Use a short timeout for scanning — we just need to know if the unit responds
        let scan_config = ClientConfig::default()
            .with_response_timeout(Duration::from_millis(1000))
            .with_retry_count(0);

        // For TCP, connect once and reuse for all unit IDs
        let addr = format!("{host}:{port}");
        let transport = match ModbusTcpTransport::connect(&addr).await {
            Ok(t) => Arc::new(ModbusTransport::Tcp(ModbusClient::with_config(
                t,
                scan_config,
            ))),
            Err(e) => {
                tracing::error!(addr, "Modbus scan: cannot connect: {e}");
                return results;
            }
        };

        tracing::info!(start_unit, end_unit, addr, "Modbus scan: probing unit IDs");

        for uid in start_unit..=end_unit {
            let unit_id = UnitId::new(uid);

            // Probe: try multiple register types — some devices only support
            // certain function codes (e.g. input registers but not holding).
            let responds = transport
                .read_holding_registers(unit_id, 0, 1)
                .await
                .is_ok()
                || transport.read_input_registers(unit_id, 0, 1).await.is_ok()
                || transport.read_coils(unit_id, 0, 1).await.is_ok();

            if !responds {
                continue;
            }

            let scan_id = format!("scan-{host}-{port}-{uid}");
            let mut info = ModbusDeviceInfo {
                instance_id: scan_id,
                host: host.to_string(),
                port,
                unit_id: uid,
                vendor: None,
                model: None,
                firmware_revision: None,
                points: Vec::new(),
            };

            // Try FC43 for device identification (swallow errors)
            if let Ok(resp) = transport
                .read_device_identification(unit_id, 0x01, 0x00)
                .await
            {
                for obj in &resp.objects {
                    let val = String::from_utf8_lossy(&obj.value).to_string();
                    match obj.object_id {
                        0x00 => info.vendor = Some(val),
                        0x01 => info.model = Some(val),
                        0x02 => info.firmware_revision = Some(val),
                        _ => {}
                    }
                }
            }

            tracing::info!(unit_id = uid, vendor = ?info.vendor, "Modbus scan: unit responding");
            results.push(info);
        }

        tracing::info!(count = results.len(), "Modbus scan: devices found");
        results
    }

    /// Scan unit IDs on the RTU bus (if configured).
    /// Uses the shared RTU transport from the first connected device.
    pub async fn scan_rtu_unit_ids(&self, start_unit: u8, end_unit: u8) -> Vec<ModbusDeviceInfo> {
        let mut results = Vec::new();

        // Find any existing RTU client to reuse
        let transport = match self.clients.values().next() {
            Some(t) => t.clone(),
            None => {
                tracing::error!("Modbus RTU scan: no RTU transport available");
                return results;
            }
        };

        let is_rtu = self
            .network_config
            .as_ref()
            .and_then(|c| c.mode.as_deref())
            .map(|m| m == "rtu")
            .unwrap_or(false);

        if !is_rtu {
            tracing::error!("Modbus RTU scan: bridge is not in RTU mode");
            return results;
        }

        let serial_port = self
            .network_config
            .as_ref()
            .and_then(|c| c.serial_port.clone())
            .unwrap_or_else(|| "rtu".to_string());

        tracing::info!(
            start_unit,
            end_unit,
            serial_port,
            "Modbus RTU scan: probing unit IDs"
        );

        for uid in start_unit..=end_unit {
            let unit_id = UnitId::new(uid);

            let responds = transport
                .read_holding_registers(unit_id, 0, 1)
                .await
                .is_ok()
                || transport.read_input_registers(unit_id, 0, 1).await.is_ok()
                || transport.read_coils(unit_id, 0, 1).await.is_ok();

            if !responds {
                continue;
            }

            let scan_id = format!("scan-rtu-{uid}");
            let mut info = ModbusDeviceInfo {
                instance_id: scan_id,
                host: serial_port.clone(),
                port: 0,
                unit_id: uid,
                vendor: None,
                model: None,
                firmware_revision: None,
                points: Vec::new(),
            };

            if let Ok(resp) = transport
                .read_device_identification(unit_id, 0x01, 0x00)
                .await
            {
                for obj in &resp.objects {
                    let val = String::from_utf8_lossy(&obj.value).to_string();
                    match obj.object_id {
                        0x00 => info.vendor = Some(val),
                        0x01 => info.model = Some(val),
                        0x02 => info.firmware_revision = Some(val),
                        _ => {}
                    }
                }
            }

            tracing::info!(unit_id = uid, vendor = ?info.vendor, "Modbus RTU scan: unit responding");
            results.push(info);
        }

        tracing::info!(count = results.len(), "Modbus RTU scan: devices found");
        results
    }

    /// Probe a device's registers to discover available points.
    ///
    /// Tests common register ranges for each function code (holding, input, coil, discrete)
    /// and creates ModbusPointInfo entries for responsive registers.
    /// Uses block reads to efficiently scan ranges.
    pub async fn probe_registers(transport: &ModbusTransport, unit_id: u8) -> Vec<ModbusPointInfo> {
        let uid = UnitId::new(unit_id);
        let mut points = Vec::new();

        // Probe strategy: try reading blocks at common starting addresses.
        // If a block responds, create a point for each register in the response.
        // Use short blocks (up to 10) to avoid overwhelming devices.
        let probe_ranges: &[(u16, u16)] = &[
            (0, 10),
            (10, 10),
            (20, 10),
            (30, 10),
            (40, 10),
            (100, 10),
            (200, 10),
            (300, 10),
            (400, 10),
            (500, 10),
            (1000, 10),
            (2000, 10),
            (3000, 10),
            (4000, 10),
        ];

        // Holding registers (FC3)
        for &(start, qty) in probe_ranges {
            if let Ok(values) = transport.read_holding_registers(uid, start, qty).await {
                for (i, _) in values.iter().enumerate() {
                    let addr = start + i as u16;
                    points.push(ModbusPointInfo {
                        point_id: format!("holding-{addr}"),
                        writable: true,
                        register_type: ModbusRegisterType::Holding,
                        address: addr,
                        data_type: None,
                        scale: None,
                    });
                }
            }
        }

        // Input registers (FC4)
        for &(start, qty) in probe_ranges {
            if let Ok(values) = transport.read_input_registers(uid, start, qty).await {
                for (i, _) in values.iter().enumerate() {
                    let addr = start + i as u16;
                    // Skip if we already have this address as a holding register
                    if points.iter().any(|p| {
                        p.address == addr && matches!(p.register_type, ModbusRegisterType::Holding)
                    }) {
                        continue;
                    }
                    points.push(ModbusPointInfo {
                        point_id: format!("input-{addr}"),
                        writable: false,
                        register_type: ModbusRegisterType::Input,
                        address: addr,
                        data_type: None,
                        scale: None,
                    });
                }
            }
        }

        // Coils (FC1) — probe smaller ranges
        let coil_ranges: &[(u16, u16)] = &[(0, 16), (16, 16), (32, 16), (100, 16), (200, 16)];
        for &(start, qty) in coil_ranges {
            if let Ok(values) = transport.read_coils(uid, start, qty).await {
                for (i, _) in values.iter().enumerate() {
                    let addr = start + i as u16;
                    points.push(ModbusPointInfo {
                        point_id: format!("coil-{addr}"),
                        writable: true,
                        register_type: ModbusRegisterType::Coil,
                        address: addr,
                        data_type: None,
                        scale: None,
                    });
                }
            }
        }

        // Discrete inputs (FC2) — probe smaller ranges
        for &(start, qty) in coil_ranges {
            if let Ok(values) = transport.read_discrete_inputs(uid, start, qty).await {
                for (i, _) in values.iter().enumerate() {
                    let addr = start + i as u16;
                    points.push(ModbusPointInfo {
                        point_id: format!("discrete-{addr}"),
                        writable: false,
                        register_type: ModbusRegisterType::DiscreteInput,
                        address: addr,
                        data_type: None,
                        scale: None,
                    });
                }
            }
        }

        tracing::info!(
            unit_id,
            point_count = points.len(),
            "Modbus probe: registers discovered"
        );
        points
    }

    /// Build a ClientConfig from device-level and network-level settings.
    fn build_client_config(&self, dev: &ModbusDeviceConfig) -> ClientConfig {
        let net = &self.network_config;
        let timeout_ms = dev
            .response_timeout_ms
            .or_else(|| net.as_ref()?.default_timeout_ms)
            .unwrap_or(5000);
        let retry = dev
            .retry_count
            .or_else(|| net.as_ref()?.default_retry_count)
            .unwrap_or(3);
        let throttle = dev.throttle_delay_ms.map(Duration::from_millis);

        ClientConfig::default()
            .with_response_timeout(Duration::from_millis(timeout_ms))
            .with_retry_count(retry)
            .with_throttle_delay(throttle)
    }

    /// Look up the persistent client for a device.
    fn get_client(&self, device_id: &str) -> Option<&Arc<ModbusTransport>> {
        self.clients.get(device_id)
    }

    /// Get an existing transport or create a new TCP connection for probing.
    pub async fn get_or_connect_transport(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Arc<ModbusTransport>, String> {
        // Check if we have an existing client whose device config matches this host:port
        let addr = format!("{host}:{port}");
        for dev in &self.devices {
            if dev.host == host && dev.port == port {
                if let Some(client) = self.clients.get(&dev.instance_id) {
                    return Ok(client.clone());
                }
            }
        }

        // Create a new connection
        let config = ClientConfig::default()
            .with_response_timeout(Duration::from_millis(2000))
            .with_retry_count(0);
        let tcp = ModbusTcpTransport::connect(&addr)
            .await
            .map_err(|e| format!("Failed to connect to {addr}: {e}"))?;
        Ok(Arc::new(ModbusTransport::Tcp(ModbusClient::with_config(
            tcp, config,
        ))))
    }

    // ── Phase 3: Advanced function codes ──

    /// FC43 — Read device identification (vendor/product/revision).
    pub async fn read_device_identification(
        &self,
        device_id: &str,
    ) -> Result<DeviceIdInfo, BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        let resp = client
            .read_device_identification(unit_id, 0x01, 0x00)
            .await
            .map_err(|e| BridgeError::Protocol(format!("FC43 failed: {e}")))?;

        let mut info = DeviceIdInfo::default();
        for obj in &resp.objects {
            let val = String::from_utf8_lossy(&obj.value).to_string();
            match obj.object_id {
                0x00 => info.vendor = Some(val),
                0x01 => info.product = Some(val),
                0x02 => info.revision = Some(val),
                _ => {}
            }
        }
        Ok(info)
    }

    /// FC8 — Diagnostics (echo test, bus counters, etc).
    pub async fn diagnostics(
        &self,
        device_id: &str,
        sub_function: u16,
        data: u16,
    ) -> Result<(u16, u16), BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        client
            .diagnostics(unit_id, sub_function, data)
            .await
            .map_err(|e| BridgeError::Protocol(format!("FC8 failed: {e}")))
    }

    /// FC22 — Mask write register (set/clear individual bits).
    pub async fn mask_write_register(
        &self,
        device_id: &str,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        client
            .mask_write_register(unit_id, address, and_mask, or_mask)
            .await
            .map_err(|e| BridgeError::Protocol(format!("FC22 failed: {e}")))
    }

    /// FC24 — Read FIFO queue.
    pub async fn read_fifo(&self, device_id: &str, address: u16) -> Result<Vec<u16>, BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        client
            .read_fifo_queue(unit_id, address)
            .await
            .map_err(|e| BridgeError::Protocol(format!("FC24 failed: {e}")))
    }

    /// FC23 — Read/write multiple registers in one transaction.
    pub async fn read_write_multiple(
        &self,
        device_id: &str,
        read_addr: u16,
        read_qty: u16,
        write_addr: u16,
        values: &[u16],
    ) -> Result<Vec<u16>, BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        client
            .read_write_multiple_registers(unit_id, read_addr, read_qty, write_addr, values)
            .await
            .map_err(|e| BridgeError::Protocol(format!("FC23 failed: {e}")))
    }

    /// Read arbitrary registers (for the register browser UI).
    pub async fn read_registers(
        &self,
        device_id: &str,
        register_type: &ModbusRegisterType,
        start: u16,
        count: u16,
    ) -> Result<Vec<u16>, BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        match register_type {
            ModbusRegisterType::Holding => client
                .read_holding_registers(unit_id, start, count)
                .await
                .map_err(|e| BridgeError::Protocol(format!("Read holding failed: {e}"))),
            ModbusRegisterType::Input => client
                .read_input_registers(unit_id, start, count)
                .await
                .map_err(|e| BridgeError::Protocol(format!("Read input failed: {e}"))),
            _ => Err(BridgeError::Protocol(
                "Use read_coils for bit registers".into(),
            )),
        }
    }

    /// Read arbitrary coils/discrete inputs (for the register browser UI).
    pub async fn read_bits(
        &self,
        device_id: &str,
        register_type: &ModbusRegisterType,
        start: u16,
        count: u16,
    ) -> Result<Vec<bool>, BridgeError> {
        let dev = self.find_device(device_id)?;
        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        match register_type {
            ModbusRegisterType::Coil => client
                .read_coils(unit_id, start, count)
                .await
                .map_err(|e| BridgeError::Protocol(format!("Read coils failed: {e}"))),
            ModbusRegisterType::DiscreteInput => client
                .read_discrete_inputs(unit_id, start, count)
                .await
                .map_err(|e| BridgeError::Protocol(format!("Read discrete failed: {e}"))),
            _ => Err(BridgeError::Protocol(
                "Use read_registers for word registers".into(),
            )),
        }
    }

    fn find_device(&self, device_id: &str) -> Result<&ModbusDeviceConfig, BridgeError> {
        self.devices
            .iter()
            .find(|d| d.instance_id == device_id)
            .ok_or_else(|| BridgeError::PointNotFound {
                device_id: device_id.to_string(),
                point_id: String::new(),
            })
    }

    /// Inject test devices for integration testing (test-only).
    /// Converts ModbusDeviceInfo back into internal ModbusDeviceConfig format.
    #[cfg(test)]
    pub fn inject_test_devices(&mut self, devices: Vec<ModbusDeviceInfo>) {
        self.devices = devices
            .into_iter()
            .map(|d| ModbusDeviceConfig {
                instance_id: d.instance_id,
                host: d.host,
                port: d.port,
                unit_id: d.unit_id,
                byte_order: ByteOrder::BigEndian,
                word_order: ByteOrder::BigEndian,
                response_timeout_ms: None,
                retry_count: None,
                throttle_delay_ms: None,
                points: d
                    .points
                    .into_iter()
                    .map(|p| ModbusPointConfig {
                        point_id: p.point_id,
                        access: if p.writable {
                            PointAccess::Output
                        } else {
                            PointAccess::Input
                        },
                        mapping: ModbusPointMapping {
                            register_type: p.register_type,
                            address: p.address,
                            data_type: p.data_type,
                            scale: p.scale,
                            register_count: None,
                            bit_offset: None,
                            bit_mask: None,
                        },
                    })
                    .collect(),
            })
            .collect();
    }
}

// ---------------------------------------------------------------------------
// PointSource implementation
// ---------------------------------------------------------------------------

impl super::traits::PointSource for ModbusBridge {
    async fn start(&mut self, store: PointStore) -> Result<(), BridgeError> {
        self.store = Some(store.clone());

        if self.devices.is_empty() {
            tracing::info!("Modbus: no devices configured");
            return Ok(());
        }

        let is_rtu = self
            .network_config
            .as_ref()
            .and_then(|c| c.mode.as_deref())
            .map(|m| m == "rtu")
            .unwrap_or(false);

        tracing::info!(
            devices = self.devices.len(),
            mode = if is_rtu { "RTU" } else { "TCP" },
            "Modbus: connecting to devices"
        );

        // For RTU mode, create a single shared transport keyed by serial port
        let mut rtu_transport: Option<Arc<ModbusTransport>> = None;

        if is_rtu {
            if let Some(ref net_config) = self.network_config {
                let serial_port = net_config.serial_port.as_deref().unwrap_or("/dev/ttyUSB0");
                let baud_rate = net_config.baud_rate.unwrap_or(9600);
                let rtu_config = ModbusRtuConfig::default();

                match ModbusRtuTransport::open(serial_port, baud_rate, rtu_config) {
                    Ok(transport) => {
                        let client_config = ClientConfig::default()
                            .with_response_timeout(Duration::from_millis(
                                net_config.default_timeout_ms.unwrap_or(5000),
                            ))
                            .with_retry_count(net_config.default_retry_count.unwrap_or(3));
                        let client = ModbusClient::with_config(transport, client_config);
                        rtu_transport = Some(Arc::new(ModbusTransport::Rtu(client)));
                        tracing::info!(serial_port, baud_rate, "Modbus: RTU bus opened");
                    }
                    Err(e) => {
                        tracing::error!(serial_port, "Modbus: RTU open failed: {e}");
                        return Err(BridgeError::ConnectionFailed(format!(
                            "RTU open failed: {e}"
                        )));
                    }
                }
            }
        }

        for dev_config in &self.devices {
            let transport = if is_rtu {
                // RTU: all devices share one serial transport
                match &rtu_transport {
                    Some(t) => t.clone(),
                    None => continue,
                }
            } else {
                // TCP: one connection per device
                let addr = format!("{}:{}", dev_config.host, dev_config.port);
                match ModbusTcpTransport::connect(&addr).await {
                    Ok(t) => {
                        let client_config = self.build_client_config(dev_config);
                        let client = ModbusClient::with_config(t, client_config);
                        Arc::new(ModbusTransport::Tcp(client))
                    }
                    Err(e) => {
                        tracing::error!(device = %dev_config.instance_id, addr, "Modbus: connection failed: {e}");
                        // Publish DeviceDown event
                        if let Some(ref bus) = self.event_bus {
                            bus.publish(Event::DeviceDown {
                                bridge_type: "modbus".into(),
                                device_key: format!("modbus-{}", dev_config.instance_id),
                            });
                        }
                        continue;
                    }
                }
            };

            // Store persistent client
            self.clients
                .insert(dev_config.instance_id.clone(), transport.clone());

            let unit_id = UnitId::new(dev_config.unit_id);

            // Initial block read of all points
            let blocks = plan_block_reads(&dev_config.points);
            let mut success_count = 0;
            let point_count = dev_config.points.len();

            for block in &blocks {
                let is_bit = matches!(
                    block.register_type,
                    ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput
                );

                if is_bit {
                    match read_bit_block(&transport, unit_id, block).await {
                        Ok(bits) => {
                            for bp in &block.points {
                                let pt = &dev_config.points[bp.point_index];
                                let val = if bp.offset < bits.len() as u16 {
                                    PointValue::Bool(bits[bp.offset as usize])
                                } else {
                                    PointValue::Bool(false)
                                };
                                let key = PointKey {
                                    device_instance_id: dev_config.instance_id.clone(),
                                    point_id: pt.point_id.clone(),
                                };
                                store.set(key, val);
                                success_count += 1;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(device = %dev_config.instance_id, "Modbus: bit block read failed: {e}");
                        }
                    }
                } else {
                    match read_register_block(&transport, unit_id, block).await {
                        Ok(regs) => {
                            for bp in &block.points {
                                let pt = &dev_config.points[bp.point_index];
                                let slice_start = bp.offset as usize;
                                let slice_end = (bp.offset + bp.reg_count) as usize;
                                if slice_end <= regs.len() {
                                    let dt = pt
                                        .mapping
                                        .data_type
                                        .clone()
                                        .unwrap_or(default_data_type(&block.register_type));
                                    let scale = pt.mapping.scale.unwrap_or(1.0);
                                    let val = decode_registers(
                                        &regs[slice_start..slice_end],
                                        &dt,
                                        scale,
                                        &dev_config.byte_order,
                                        &dev_config.word_order,
                                    );
                                    let val = apply_bit_extraction(
                                        &pt.mapping,
                                        val,
                                        &regs[slice_start..slice_end],
                                    );
                                    let key = PointKey {
                                        device_instance_id: dev_config.instance_id.clone(),
                                        point_id: pt.point_id.clone(),
                                    };
                                    store.set(key, val);
                                    success_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(device = %dev_config.instance_id, "Modbus: register block read failed: {e}");
                        }
                    }
                }
            }

            tracing::info!(device = %dev_config.instance_id, success_count, point_count, "Modbus: device connected");

            // Publish DeviceDiscovered event
            if let Some(ref bus) = self.event_bus {
                bus.publish(Event::DeviceDiscovered {
                    bridge_type: "modbus".into(),
                    device_key: format!("modbus-{}", dev_config.instance_id),
                });
            }

            // Spawn poll loop for this device (with backoff + events)
            let poll_store = store.clone();
            let poll_config = dev_config.clone();
            let poll_interval = self.poll_interval;
            let poll_transport = transport.clone();
            let poll_event_bus = self.event_bus.clone();

            let handle = tokio::spawn(async move {
                poll_device_with_backoff(
                    poll_transport,
                    UnitId::new(poll_config.unit_id),
                    poll_config,
                    poll_store,
                    poll_interval,
                    poll_event_bus,
                )
                .await;
            });
            self.poll_handles.push(handle);
        }

        let total_points: usize = self.devices.iter().map(|d| d.points.len()).sum();
        tracing::info!(
            devices = self.poll_handles.len(),
            points = total_points,
            "Modbus bridge started"
        );

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), BridgeError> {
        for h in self.poll_handles.drain(..) {
            h.abort();
        }
        self.clients.clear();
        Ok(())
    }

    async fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        _priority: Option<u8>,
    ) -> Result<(), BridgeError> {
        let dev = self.find_device(device_id)?;

        let pt = dev
            .points
            .iter()
            .find(|p| p.point_id == point_id)
            .ok_or_else(|| BridgeError::PointNotFound {
                device_id: device_id.to_string(),
                point_id: point_id.to_string(),
            })?;

        if matches!(pt.access, PointAccess::Input) {
            return Err(BridgeError::WriteRejected(format!(
                "Point {} is read-only (input)",
                point_id
            )));
        }

        let client = self
            .get_client(device_id)
            .ok_or_else(|| BridgeError::ConnectionFailed(format!("No client for {device_id}")))?;
        let unit_id = UnitId::new(dev.unit_id);

        // If point has bit_mask, use FC22 mask write instead
        if let Some(bit_mask) = pt
            .mapping
            .bit_mask
            .filter(|_| matches!(pt.mapping.register_type, ModbusRegisterType::Holding))
        {
            let bool_val = match &value {
                PointValue::Bool(b) => *b,
                PointValue::Integer(i) => *i != 0,
                PointValue::Float(f) => *f != 0.0,
            };
            let (and_mask, or_mask) = if bool_val {
                (0xFFFF, bit_mask) // set masked bits
            } else {
                (!bit_mask, 0x0000) // clear masked bits
            };
            client
                .mask_write_register(unit_id, pt.mapping.address, and_mask, or_mask)
                .await
                .map_err(|e| BridgeError::Protocol(format!("FC22 write failed: {e}")))?;
        } else {
            write_point(
                client,
                unit_id,
                &pt.mapping,
                &value,
                &dev.byte_order,
                &dev.word_order,
            )
            .await
            .map_err(|e| BridgeError::Protocol(format!("Modbus write failed: {e}")))?;
        }

        // Write-back verification: read the register back and confirm the value was accepted
        match read_point(
            client,
            unit_id,
            &pt.mapping,
            &dev.byte_order,
            &dev.word_order,
        )
        .await
        {
            Ok(read_back) => {
                // Update PointStore with actual device value (may differ from requested)
                if let Some(store) = &self.store {
                    store.set(
                        PointKey {
                            device_instance_id: device_id.to_string(),
                            point_id: point_id.to_string(),
                        },
                        read_back,
                    );
                }
            }
            Err(_) => {
                // Read-back failed; use the written value as best-effort
                if let Some(store) = &self.store {
                    store.set(
                        PointKey {
                            device_instance_id: device_id.to_string(),
                            point_id: point_id.to_string(),
                        },
                        value,
                    );
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Poll loop with backoff and EventBus integration
// ---------------------------------------------------------------------------

async fn poll_device_with_backoff(
    transport: Arc<ModbusTransport>,
    unit_id: UnitId,
    config: ModbusDeviceConfig,
    store: PointStore,
    interval: Duration,
    event_bus: Option<EventBus>,
) {
    let mut backoff = DeviceBackoff::new();
    let device_key = format!("modbus-{}", config.instance_id);
    let blocks = plan_block_reads(&config.points);

    loop {
        tokio::time::sleep(interval).await;

        if backoff.should_skip() {
            continue;
        }

        let mut any_error = false;

        for block in &blocks {
            let is_bit = matches!(
                block.register_type,
                ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput
            );

            if is_bit {
                match read_bit_block(&transport, unit_id, block).await {
                    Ok(bits) => {
                        // Warn if device returned fewer bits than requested
                        if bits.len() < block.count as usize {
                            tracing::warn!(
                                device = %config.instance_id,
                                got = bits.len(),
                                expected = block.count,
                                addr = block.start_address,
                                "Modbus: short bit response"
                            );
                        }
                        for bp in &block.points {
                            let pt = &config.points[bp.point_index];
                            let key = PointKey {
                                device_instance_id: config.instance_id.clone(),
                                point_id: pt.point_id.clone(),
                            };
                            if (bp.offset as usize) < bits.len() {
                                let val = PointValue::Bool(bits[bp.offset as usize]);
                                store.set_if_changed(key.clone(), val);
                                store.clear_status(&key, PointStatusFlags::FAULT);
                            } else {
                                // Point fell outside short response — mark FAULT
                                store.set_status(&key, PointStatusFlags::FAULT);
                            }
                        }
                    }
                    Err(e) => {
                        let level = if e.is_transport() {
                            "transport"
                        } else if e.is_busy() {
                            "busy"
                        } else {
                            "exception"
                        };
                        tracing::warn!(device = %config.instance_id, level, "Modbus: bit block poll error: {e}");
                        for bp in &block.points {
                            let pt = &config.points[bp.point_index];
                            let key = PointKey {
                                device_instance_id: config.instance_id.clone(),
                                point_id: pt.point_id.clone(),
                            };
                            store.set_status(&key, PointStatusFlags::FAULT);
                        }
                        any_error = true;
                    }
                }
            } else {
                match read_register_block(&transport, unit_id, block).await {
                    Ok(regs) => {
                        // Warn if device returned fewer registers than requested
                        if regs.len() < block.count as usize {
                            tracing::warn!(
                                device = %config.instance_id,
                                got = regs.len(),
                                expected = block.count,
                                addr = block.start_address,
                                "Modbus: short register response"
                            );
                        }
                        for bp in &block.points {
                            let pt = &config.points[bp.point_index];
                            let slice_start = bp.offset as usize;
                            let slice_end = (bp.offset + bp.reg_count) as usize;
                            let key = PointKey {
                                device_instance_id: config.instance_id.clone(),
                                point_id: pt.point_id.clone(),
                            };
                            if slice_end <= regs.len() {
                                let dt = pt
                                    .mapping
                                    .data_type
                                    .clone()
                                    .unwrap_or(default_data_type(&block.register_type));
                                let scale = pt.mapping.scale.unwrap_or(1.0);
                                let val = decode_registers(
                                    &regs[slice_start..slice_end],
                                    &dt,
                                    scale,
                                    &config.byte_order,
                                    &config.word_order,
                                );
                                let val = apply_bit_extraction(
                                    &pt.mapping,
                                    val,
                                    &regs[slice_start..slice_end],
                                );
                                store.set_if_changed(key.clone(), val);
                                store.clear_status(&key, PointStatusFlags::FAULT);
                            } else {
                                // Point fell outside short response — mark FAULT
                                store.set_status(&key, PointStatusFlags::FAULT);
                            }
                        }
                    }
                    Err(e) => {
                        let level = if e.is_transport() {
                            "transport"
                        } else if e.is_busy() {
                            "busy"
                        } else {
                            "exception"
                        };
                        tracing::warn!(device = %config.instance_id, level, "Modbus: register block poll error: {e}");
                        for bp in &block.points {
                            let pt = &config.points[bp.point_index];
                            let key = PointKey {
                                device_instance_id: config.instance_id.clone(),
                                point_id: pt.point_id.clone(),
                            };
                            store.set_status(&key, PointStatusFlags::FAULT);
                        }
                        any_error = true;
                    }
                }
            }
        }

        // Backoff + event handling
        if any_error {
            backoff.record_failure();
            if backoff.is_down() && !backoff.was_down {
                backoff.was_down = true;
                // Mark all points as DOWN
                for pt in &config.points {
                    let key = PointKey {
                        device_instance_id: config.instance_id.clone(),
                        point_id: pt.point_id.clone(),
                    };
                    store.set_status(&key, PointStatusFlags::DOWN);
                }
                if let Some(ref bus) = event_bus {
                    bus.publish(Event::DeviceDown {
                        bridge_type: "modbus".into(),
                        device_key: device_key.clone(),
                    });
                }
            }
        } else {
            if backoff.was_down {
                backoff.was_down = false;
                // Clear DOWN from all points
                for pt in &config.points {
                    let key = PointKey {
                        device_instance_id: config.instance_id.clone(),
                        point_id: pt.point_id.clone(),
                    };
                    store.clear_status(&key, PointStatusFlags::DOWN);
                }
                if let Some(ref bus) = event_bus {
                    bus.publish(Event::DeviceRecovered {
                        bridge_type: "modbus".into(),
                        device_key: device_key.clone(),
                    });
                }
            }
            backoff.record_success();
        }
    }
}

// ---------------------------------------------------------------------------
// Block read helpers
// ---------------------------------------------------------------------------

async fn read_register_block(
    transport: &ModbusTransport,
    unit_id: UnitId,
    block: &RegisterBlock,
) -> Result<Vec<u16>, ModbusError> {
    match block.register_type {
        ModbusRegisterType::Holding => {
            transport
                .read_holding_registers(unit_id, block.start_address, block.count)
                .await
        }
        ModbusRegisterType::Input => {
            transport
                .read_input_registers(unit_id, block.start_address, block.count)
                .await
        }
        _ => Err(ModbusError::Encode("Not a register block".to_string())),
    }
}

async fn read_bit_block(
    transport: &ModbusTransport,
    unit_id: UnitId,
    block: &RegisterBlock,
) -> Result<Vec<bool>, ModbusError> {
    match block.register_type {
        ModbusRegisterType::Coil => {
            transport
                .read_coils(unit_id, block.start_address, block.count)
                .await
        }
        ModbusRegisterType::DiscreteInput => {
            transport
                .read_discrete_inputs(unit_id, block.start_address, block.count)
                .await
        }
        _ => Err(ModbusError::Encode("Not a bit block".to_string())),
    }
}

// ---------------------------------------------------------------------------
// Bit extraction (Phase 2B)
// ---------------------------------------------------------------------------

/// If the mapping has bit_offset or bit_mask, extract accordingly.
fn apply_bit_extraction(
    mapping: &ModbusPointMapping,
    decoded: PointValue,
    raw_regs: &[u16],
) -> PointValue {
    let raw_u16 = raw_regs.first().copied().unwrap_or(0);

    if let Some(bit_offset) = mapping.bit_offset {
        return PointValue::Bool((raw_u16 >> bit_offset) & 1 != 0);
    }

    if let Some(bit_mask) = mapping.bit_mask {
        return PointValue::Bool(raw_u16 & bit_mask != 0);
    }

    decoded
}

// ---------------------------------------------------------------------------
// Register read/write helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
async fn read_point(
    client: &ModbusTransport,
    unit_id: UnitId,
    mapping: &ModbusPointMapping,
    byte_order: &ByteOrder,
    word_order: &ByteOrder,
) -> Result<PointValue, ModbusError> {
    let data_type = mapping
        .data_type
        .clone()
        .unwrap_or(default_data_type(&mapping.register_type));
    let scale = mapping.scale.unwrap_or(1.0);

    match mapping.register_type {
        ModbusRegisterType::Coil => {
            let values = client.read_coils(unit_id, mapping.address, 1).await?;
            Ok(PointValue::Bool(values[0]))
        }
        ModbusRegisterType::DiscreteInput => {
            let values = client
                .read_discrete_inputs(unit_id, mapping.address, 1)
                .await?;
            Ok(PointValue::Bool(values[0]))
        }
        ModbusRegisterType::Holding => {
            let reg_count = register_count(&data_type, mapping.register_count);
            let regs = client
                .read_holding_registers(unit_id, mapping.address, reg_count)
                .await?;
            let val = decode_registers(&regs, &data_type, scale, byte_order, word_order);
            Ok(apply_bit_extraction(mapping, val, &regs))
        }
        ModbusRegisterType::Input => {
            let reg_count = register_count(&data_type, mapping.register_count);
            let regs = client
                .read_input_registers(unit_id, mapping.address, reg_count)
                .await?;
            let val = decode_registers(&regs, &data_type, scale, byte_order, word_order);
            Ok(apply_bit_extraction(mapping, val, &regs))
        }
    }
}

async fn write_point(
    client: &ModbusTransport,
    unit_id: UnitId,
    mapping: &ModbusPointMapping,
    value: &PointValue,
    byte_order: &ByteOrder,
    word_order: &ByteOrder,
) -> Result<(), ModbusError> {
    let data_type = mapping
        .data_type
        .clone()
        .unwrap_or(default_data_type(&mapping.register_type));
    let scale = mapping.scale.unwrap_or(1.0);

    match mapping.register_type {
        ModbusRegisterType::Coil => {
            let bool_val = match value {
                PointValue::Bool(b) => *b,
                PointValue::Integer(i) => *i != 0,
                PointValue::Float(f) => *f != 0.0,
            };
            client
                .write_single_coil(unit_id, mapping.address, bool_val)
                .await
        }
        ModbusRegisterType::Holding => {
            let regs = encode_registers(value, &data_type, scale, byte_order, word_order);
            if regs.len() == 1 {
                client
                    .write_single_register(unit_id, mapping.address, regs[0])
                    .await
            } else {
                client
                    .write_multiple_registers(unit_id, mapping.address, &regs)
                    .await
            }
        }
        ModbusRegisterType::DiscreteInput | ModbusRegisterType::Input => Err(ModbusError::Encode(
            "Cannot write to input registers".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Data conversion helpers
// ---------------------------------------------------------------------------

fn default_data_type(reg_type: &ModbusRegisterType) -> ModbusDataType {
    match reg_type {
        ModbusRegisterType::Coil | ModbusRegisterType::DiscreteInput => ModbusDataType::Bool,
        ModbusRegisterType::Holding | ModbusRegisterType::Input => ModbusDataType::Uint16,
    }
}

fn register_count(data_type: &ModbusDataType, explicit: Option<u16>) -> u16 {
    explicit.unwrap_or(match data_type {
        ModbusDataType::Bool | ModbusDataType::Uint16 | ModbusDataType::Int16 => 1,
        ModbusDataType::Uint32 | ModbusDataType::Int32 | ModbusDataType::Float32 => 2,
        ModbusDataType::Float64 => 4,
    })
}

fn decode_registers(
    regs: &[u16],
    data_type: &ModbusDataType,
    scale: f64,
    byte_order: &ByteOrder,
    word_order: &ByteOrder,
) -> PointValue {
    // Apply byte-swap within each register if device uses little-endian byte order
    let regs: Vec<u16> = if matches!(byte_order, ByteOrder::LittleEndian) {
        regs.iter().map(|r| r.swap_bytes()).collect()
    } else {
        regs.to_vec()
    };
    let regs = &regs;
    match data_type {
        ModbusDataType::Bool => PointValue::Bool(regs.first().copied().unwrap_or(0) != 0),
        ModbusDataType::Uint16 => {
            let raw = regs.first().copied().unwrap_or(0) as f64;
            PointValue::Float(raw / scale)
        }
        ModbusDataType::Int16 => {
            let raw = regs.first().copied().unwrap_or(0) as i16 as f64;
            PointValue::Float(raw / scale)
        }
        ModbusDataType::Uint32 => {
            let raw = assemble_u32(regs, word_order) as f64;
            PointValue::Float(raw / scale)
        }
        ModbusDataType::Int32 => {
            let raw = assemble_u32(regs, word_order) as i32 as f64;
            PointValue::Float(raw / scale)
        }
        ModbusDataType::Float32 => {
            let bits = assemble_u32(regs, word_order);
            let raw = f32::from_bits(bits) as f64;
            PointValue::Float(raw / scale)
        }
        ModbusDataType::Float64 => {
            let bits = assemble_u64(regs, word_order);
            let raw = f64::from_bits(bits);
            PointValue::Float(raw / scale)
        }
    }
}

fn encode_registers(
    value: &PointValue,
    data_type: &ModbusDataType,
    scale: f64,
    byte_order: &ByteOrder,
    word_order: &ByteOrder,
) -> Vec<u16> {
    let raw = value.as_f64() * scale;

    let mut regs = match data_type {
        ModbusDataType::Bool => vec![if raw != 0.0 { 1 } else { 0 }],
        ModbusDataType::Uint16 => vec![raw as u16],
        ModbusDataType::Int16 => vec![(raw as i16) as u16],
        ModbusDataType::Uint32 => split_u32(raw as u32, word_order),
        ModbusDataType::Int32 => split_u32((raw as i32) as u32, word_order),
        ModbusDataType::Float32 => split_u32((raw as f32).to_bits(), word_order),
        ModbusDataType::Float64 => split_u64(raw.to_bits(), word_order),
    };

    // Apply byte-swap within each register if device uses little-endian byte order
    if matches!(byte_order, ByteOrder::LittleEndian) {
        for r in &mut regs {
            *r = r.swap_bytes();
        }
    }

    regs
}

/// Assemble two u16 registers into a u32, respecting word order.
fn assemble_u32(regs: &[u16], word_order: &ByteOrder) -> u32 {
    if regs.len() < 2 {
        return regs.first().copied().unwrap_or(0) as u32;
    }
    match word_order {
        ByteOrder::BigEndian => (regs[0] as u32) << 16 | regs[1] as u32,
        ByteOrder::LittleEndian => (regs[1] as u32) << 16 | regs[0] as u32,
    }
}

/// Assemble four u16 registers into a u64, respecting word order.
fn assemble_u64(regs: &[u16], word_order: &ByteOrder) -> u64 {
    if regs.len() < 4 {
        return assemble_u32(regs, word_order) as u64;
    }
    match word_order {
        ByteOrder::BigEndian => {
            (regs[0] as u64) << 48
                | (regs[1] as u64) << 32
                | (regs[2] as u64) << 16
                | regs[3] as u64
        }
        ByteOrder::LittleEndian => {
            (regs[3] as u64) << 48
                | (regs[2] as u64) << 32
                | (regs[1] as u64) << 16
                | regs[0] as u64
        }
    }
}

/// Split a u32 into two u16 registers, respecting word order.
fn split_u32(val: u32, word_order: &ByteOrder) -> Vec<u16> {
    let hi = (val >> 16) as u16;
    let lo = val as u16;
    match word_order {
        ByteOrder::BigEndian => vec![hi, lo],
        ByteOrder::LittleEndian => vec![lo, hi],
    }
}

/// Split a u64 into four u16 registers, respecting word order.
fn split_u64(val: u64, word_order: &ByteOrder) -> Vec<u16> {
    let w0 = (val >> 48) as u16;
    let w1 = (val >> 32) as u16;
    let w2 = (val >> 16) as u16;
    let w3 = val as u16;
    match word_order {
        ByteOrder::BigEndian => vec![w0, w1, w2, w3],
        ByteOrder::LittleEndian => vec![w3, w2, w1, w0],
    }
}

// ---------------------------------------------------------------------------
// ProtocolBridgeHandle — generic bridge interface
// ---------------------------------------------------------------------------

impl crate::plugin::ProtocolBridgeHandle for ModbusBridge {
    fn write_point(
        &self,
        device_id: &str,
        point_id: &str,
        value: PointValue,
        priority: Option<u8>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), super::traits::BridgeError>> + Send + '_>,
    > {
        let device_id = device_id.to_string();
        let point_id = point_id.to_string();
        Box::pin(async move {
            super::traits::PointSource::write_point(self, &device_id, &point_id, value, priority)
                .await
        })
    }

    fn stop(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<(), super::traits::BridgeError>> + Send + '_>,
    > {
        Box::pin(super::traits::PointSource::stop(self))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile::ModbusPointMapping;

    #[test]
    fn plan_block_reads_contiguous_merge() {
        let points = vec![
            ModbusPointConfig {
                point_id: "a".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 100,
                    data_type: Some(ModbusDataType::Uint16),
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
            ModbusPointConfig {
                point_id: "b".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 101,
                    data_type: Some(ModbusDataType::Uint16),
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
            ModbusPointConfig {
                point_id: "c".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 102,
                    data_type: Some(ModbusDataType::Float32),
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
        ];

        let blocks = plan_block_reads(&points);
        assert_eq!(
            blocks.len(),
            1,
            "contiguous registers should merge into 1 block"
        );
        assert_eq!(blocks[0].start_address, 100);
        assert_eq!(blocks[0].count, 4); // 1 + 1 + 2 (float32)
        assert_eq!(blocks[0].points.len(), 3);
    }

    #[test]
    fn plan_block_reads_gap_split() {
        let points = vec![
            ModbusPointConfig {
                point_id: "a".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 0,
                    data_type: Some(ModbusDataType::Uint16),
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
            ModbusPointConfig {
                point_id: "b".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 100,
                    data_type: Some(ModbusDataType::Uint16),
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
        ];

        let blocks = plan_block_reads(&points);
        assert_eq!(blocks.len(), 2, "large gap should split into 2 blocks");
    }

    #[test]
    fn plan_block_reads_mixed_types() {
        let points = vec![
            ModbusPointConfig {
                point_id: "coil".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Coil,
                    address: 0,
                    data_type: None,
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
            ModbusPointConfig {
                point_id: "holding".into(),
                access: PointAccess::Input,
                mapping: ModbusPointMapping {
                    register_type: ModbusRegisterType::Holding,
                    address: 0,
                    data_type: None,
                    scale: None,
                    register_count: None,
                    bit_offset: None,
                    bit_mask: None,
                },
            },
        ];

        let blocks = plan_block_reads(&points);
        assert_eq!(
            blocks.len(),
            2,
            "different register types should be separate blocks"
        );
    }

    #[test]
    fn bit_offset_extraction() {
        let mapping = ModbusPointMapping {
            register_type: ModbusRegisterType::Holding,
            address: 0,
            data_type: Some(ModbusDataType::Uint16),
            scale: None,
            register_count: None,
            bit_offset: Some(3),
            bit_mask: None,
        };

        // Bit 3 is set (0b1000 = 8)
        let val = apply_bit_extraction(&mapping, PointValue::Float(8.0), &[0x0008]);
        assert!(matches!(val, PointValue::Bool(true)));

        // Bit 3 is not set
        let val = apply_bit_extraction(&mapping, PointValue::Float(4.0), &[0x0004]);
        assert!(matches!(val, PointValue::Bool(false)));
    }

    #[test]
    fn bit_mask_extraction() {
        let mapping = ModbusPointMapping {
            register_type: ModbusRegisterType::Holding,
            address: 0,
            data_type: Some(ModbusDataType::Uint16),
            scale: None,
            register_count: None,
            bit_offset: None,
            bit_mask: Some(0x00F0),
        };

        // Bits 4-7 set
        let val = apply_bit_extraction(&mapping, PointValue::Float(0.0), &[0x00F0]);
        assert!(matches!(val, PointValue::Bool(true)));

        // None of bits 4-7 set
        let val = apply_bit_extraction(&mapping, PointValue::Float(0.0), &[0x000F]);
        assert!(matches!(val, PointValue::Bool(false)));
    }

    #[test]
    fn no_bit_extraction_passthrough() {
        let mapping = ModbusPointMapping {
            register_type: ModbusRegisterType::Holding,
            address: 0,
            data_type: Some(ModbusDataType::Uint16),
            scale: None,
            register_count: None,
            bit_offset: None,
            bit_mask: None,
        };

        let val = apply_bit_extraction(&mapping, PointValue::Float(42.0), &[42]);
        assert!(matches!(val, PointValue::Float(f) if (f - 42.0).abs() < f64::EPSILON));
    }
}
