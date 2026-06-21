use std::fmt;

/// Command and register definitions for eMMC SPI protocol
use crate::prelude::*;

pub const RCA: u32 = 10;
pub const RCA_ARG: u32 = RCA << 16;
pub const BLOCK_SIZE: usize = 512;
pub const EXT_CSD_SIZE: usize = 512;
pub const BASE_CLOCK_MHZ: f64 = 196.875;

// NOR
pub const NOR_PAGE_SIZE: usize = 256;
pub const NOR_SECTOR_SIZE: usize = 4096;

/// SPI Command type (2 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TransferOp {
    /// Read operation (0b01)
    Read = 0x1,
    /// Write operation (0b10)
    Write = 0x2,
}

impl TransferOp {
    /// Get the 2-bit command value
    pub fn bits(self) -> u8 {
        self as u8
    }

    /// Get number of bits for this command field (always 2)
    pub const fn bit_length() -> u8 {
        2
    }
}

/// eMMC SPI Controller Register addresses (8 bits)
//
//   0x01 → Block Size / Block Count         0x09 → Present State
//   0x02 → Argument                         0x0A → Host Control 1
//   0x03 → Transfer Mode + Command          0x0B → Clock Control
//   0x04 → Response [31:0]                  0x0C → Interrupt Status
//   0x05 → Response [63:32]                 0x0D → Int Status Enable
//   0x06 → Response [95:64]                 0x0E → Int Signal Enable
//   0x07 → Response [127:96]                0x0F → Auto CMD / Host Ctrl 2
//   0x08 → Data FIFO
//
// Vendor: 0x86 = tuning trigger, 0x88 = XIP output delay
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    /// Register 0x01
    BlockSizeCount = 0x01,
    /// Argument register - buffer/FIFO config
    Argument = 0x02,
    /// Command and Transfer Mode register
    CommandAndTransferMode = 0x03,
    /// Response/Status register (also used for status polling)
    Response0And1 = 0x04,
    Response2And3 = 0x05,
    Response4And5 = 0x06,
    Response6And7 = 0x07,
    /// Data FIFO register (for 512-byte block reads)
    DataFifo = 0x08,
    /// Present State register
    PresentState = 0x09,
    /// Register 0x0A
    HostControl = 0x0A,
    /// Clock control
    ClockControl = 0x0B,
    /// InterruptStatus
    InterruptStatus = 0x0C,
    /// Configuration register 1
    InterruptStatusEn = 0x0D,
    /// Configuration register 2
    InterruptSignalEn = 0x0E,
    /// Register 0x0F
    AutoCmdHost2 = 0x0F,
    /// Initialization command register
    InitCommand = 0x44,
    /// Vendor tuning
    VendorTuning = 0x86,
    /// Register 0x88
    XipOutputDelay = 0x88,
    // Xip Data, first register
    XipDataFirst = 0xC0,
    // Xip Data, last register
    XipDataLast = 0xCD,
}

impl From<Register> for u8 {
    fn from(val: Register) -> Self {
        val as u8
    }
}

impl Register {
    /// Get the 8-bit register address
    pub fn address(self) -> u8 {
        self as u8
    }

    /// Get number of bits for register address field (always 8)
    pub const fn bit_length() -> u8 {
        8
    }

    /// Create from raw address value
    pub fn from_address(addr: u8) -> Option<Self> {
        match addr {
            0x01 => Some(Self::BlockSizeCount),
            0x02 => Some(Self::Argument),
            0x03 => Some(Self::CommandAndTransferMode),
            0x04 => Some(Self::Response0And1),
            0x05 => Some(Self::Response2And3),
            0x06 => Some(Self::Response4And5),
            0x07 => Some(Self::Response6And7),
            0x08 => Some(Self::DataFifo),
            0x09 => Some(Self::PresentState),
            0x0A => Some(Self::HostControl),
            0x0B => Some(Self::ClockControl),
            0x0C => Some(Self::InterruptStatus),
            0x0D => Some(Self::InterruptStatusEn),
            0x0E => Some(Self::InterruptSignalEn),
            0x0F => Some(Self::AutoCmdHost2),
            0x44 => Some(Self::InitCommand),
            0x86 => Some(Self::VendorTuning),
            0x88 => Some(Self::XipOutputDelay),
            0xC0 => Some(Self::XipDataFirst),
            0xCD => Some(Self::XipDataLast),
            _ => None,
        }
    }
}

pub mod responses {
    pub const RESP_NONE: u8 = 0x00;
    pub const RESP_R2: u8 = 0x09; // 136-bit
    pub const RESP_R3: u8 = 0x02; // 48-bit, no CRC/Index
    pub const RESP_R1: u8 = 0x1A; // 48-bit, CRC+Index
    pub const RESP_R1B: u8 = 0x1B; // 48-bit, CRC+Index, busy
}

/// MMC command encoding
///
/// Packed u32: upper 16 = Command Register, lower 16 = Transfer Mode.
///   Command Register: [13:8] index, [5] data-present, [4] index-check,
///                     [3] CRC-check, [1:0] response type
pub const fn make_cmd(index: u8, resp: u8) -> u32 {
    ((index as u32) << 24) | ((resp as u32) << 16)
}

pub mod commands {
    use super::{make_cmd, responses::*};

    // Non-data commands
    pub const CMD0_GO_IDLE: u32 = make_cmd(0, RESP_NONE);
    pub const CMD1_SEND_OP_COND: u32 = make_cmd(1, RESP_R3);
    pub const CMD2_ALL_SEND_CID: u32 = make_cmd(2, RESP_R2);
    pub const CMD3_SET_RCA: u32 = make_cmd(3, RESP_R1);
    pub const CMD6_SWITCH: u32 = make_cmd(6, RESP_R1B);
    pub const CMD7_SELECT_CARD: u32 = make_cmd(7, RESP_R1);
    pub const CMD7_DESELECT_CARD: u32 = make_cmd(7, 0x18);
    pub const CMD9_SEND_CSD: u32 = make_cmd(9, RESP_R2); // single read
    pub const CMD12_STOP_READ: u32 = make_cmd(12, RESP_R1);
    pub const CMD12_STOP_WRITE: u32 = make_cmd(12, RESP_R1B);
    pub const CMD13_SEND_STATUS: u32 = make_cmd(13, RESP_R1);
    pub const CMD16_SET_BLOCKLEN: u32 = make_cmd(16, RESP_R1);
    pub const CMD35_ERASE_GROUP_START: u32 = make_cmd(35, RESP_R1);
    pub const CMD36_ERASE_GROUP_END: u32 = make_cmd(36, RESP_R1);
    pub const CMD38_ERASE: u32 = make_cmd(38, RESP_R1);

    // Data transfer commands: CMD Register (upper 16) | Transfer Mode (lower 16)
    //   Transfer Mode bits: [5] multi-block, [4] read-direction, [2] auto-CMD12,
    //                       [1] block-count-enable
    pub const CMD8_SEND_EXT_CSD: u32 = 0x083A_0010; // single read
    pub const CMD17_READ_SINGLE_BLOCK: u32 = 0x113A_0010;
    pub const CMD18_READ_MULTIPLE_BLOCK: u32 = 0x123A_0036;
    pub const CMD24_WRITE_BLOCK: u32 = 0x183A_0000;
    pub const CMD25_WRITE_MULTIPLE_BLOCK: u32 = 0x193A_0026;
}

/// Erase type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseType {
    Erase = 0,
    Trim = 1,
    Discard = 3,
}

/// Data size for register operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSize {
    /// Standard register size (4 bytes)
    Register = 4,
    /// Block size for eMMC data blocks (512 bytes)
    Page = 512,
}

impl DataSize {
    /// Get the size in bytes
    pub fn bytes(self) -> usize {
        self as usize
    }
}

/// eMMC State enum (from Present State register bits 9-12)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MmcState {
    Idle = 0,
    Ready = 1,
    Ident = 2,
    Standby = 3,
    Transfer = 4,
    Data = 5,
    Receive = 6,
    Program = 7,
    Disabled = 8,
    _BTDST = 9,
    Sleep = 10,
}

impl MmcState {
    /// Parse state from status bits
    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits & 0x0F {
            0 => Some(Self::Idle),
            1 => Some(Self::Ready),
            2 => Some(Self::Ident),
            3 => Some(Self::Standby),
            4 => Some(Self::Transfer),
            5 => Some(Self::Data),
            6 => Some(Self::Receive),
            7 => Some(Self::Program),
            8 => Some(Self::Disabled),
            9 => Some(Self::_BTDST),
            10 => Some(Self::Sleep),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManufacturerId {
    Samsung,
    Hynix,
    Toshiba,
    Unknown(u8),
}

impl ManufacturerId {
    fn from_mid(mid: u8) -> Self {
        match mid {
            0x15 => Self::Samsung,
            0x90 => Self::Hynix,
            0x11 => Self::Toshiba,
            x => Self::Unknown(x),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmmcCid {
    pub mid: u8,
    pub manufacturer: ManufacturerId,
    pub cbx: u8,
    pub pnm: String,
    pub prv: u8,
    pub psn: u32,
}

impl EmmcCid {
    pub fn from_response(resp: [u32; 4]) -> Self {
        let r0 = resp[0];
        let r1 = resp[1];
        let r2 = resp[2];
        let r3 = resp[3];

        let mid = ((r3 >> 16) & 0xff) as u8;
        let cbx = ((r3 >> 8) & 0x03) as u8;

        let pnm_bytes = [
            (r2 >> 24) as u8,
            (r2 >> 16) as u8,
            (r2 >> 8) as u8,
            r2 as u8,
            (r1 >> 24) as u8,
            (r1 >> 16) as u8,
        ];

        let pnm = pnm_bytes
            .into_iter()
            .map(char::from)
            .collect::<String>();

        let prv = ((r1 >> 8) & 0xff) as u8;

        let psn =
            ((r1 & 0xff) << 24) |
            (((r0 >> 24) & 0xff) << 16) |
            (((r0 >> 16) & 0xff) << 8) |
            ((r0 >> 8) & 0xff);

        Self {
            mid,
            manufacturer: ManufacturerId::from_mid(mid),
            cbx,
            pnm,
            prv,
            psn,
        }
    }

    pub fn hw_revision(&self) -> u8 {
        self.prv >> 4
    }

    pub fn fw_revision(&self) -> u8 {
        self.prv & 0x0f
    }
}

impl fmt::Display for EmmcCid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Manufacturer : {:?}", self.manufacturer)?;
        writeln!(f, "MID          : 0x{:02X}", self.mid)?;
        writeln!(f, "CBX          : {}", self.cbx)?;
        writeln!(f, "Product      : {}", self.pnm)?;
        writeln!(
            f,
            "Revision     : {}.{} (0x{:02X})",
            self.hw_revision(),
            self.fw_revision(),
            self.prv
        )?;
        writeln!(f, "Serial       : 0x{:08X}", self.psn)
    }
}

#[derive(Debug, Clone)]
pub struct EmmcCsd {
    raw: u128,
    /// CSD_STRUCTURE
    pub csd_structure: u8,
    /// SPEC_VERS
    pub spec_vers: u8,
    /// Command classes
    pub ccc: u16,
    /// Maximum transfer speed field
    pub tran_speed: u8,
    /// Read block length (2^n)
    pub read_bl_len: u8,
    /// Write block length (2^n)
    pub write_bl_len: u8,
}

impl EmmcCsd {
    pub fn from_response(resp: [u32; 4]) -> Self {
        // NOTE / IMPORTANT: Assemble 4x u32 into u128 value and shift up by 1 byte
        let csd =
              (((resp[3] as u128) << 96)
            | ((resp[2] as u128) << 64)
            | ((resp[1] as u128) << 32)
            |  (resp[0] as u128))
            << 8;

        fn bits(v: u128, hi: u32, lo: u32) -> u32 {
            ((v >> lo) & ((1u128 << (hi - lo + 1)) - 1)) as u32
        }
        
        Self {
            raw: csd,
            csd_structure: bits(csd,127,126) as u8,
            spec_vers: bits(csd,125,122) as u8,
            tran_speed: bits(csd,103,96) as u8,
            ccc: bits(csd,95,84) as u16,
            read_bl_len: bits(csd,83,80) as u8,
            write_bl_len: bits(csd,25,22) as u8,
        }
    }

    pub fn as_u128(&self) -> u128 {
        self.raw
    }

    pub fn read_block_size(&self) -> u32 {
        1u32 << self.read_bl_len
    }

    pub fn write_block_size(&self) -> u32 {
        1u32 << self.write_bl_len
    }

    pub fn tran_speed_hz(&self) -> Option<u32> {
        // JEDEC/MMC TRAN_SPEED decoding
        const MULT: [u32; 16] = [
            0, 10, 12, 13, 15, 20, 26, 30,
            35, 40, 45, 52, 55, 60, 70, 80,
        ];

        const UNIT: [u32; 8] = [
            100_000,
            1_000_000,
            10_000_000,
            100_000_000,
            0, 0, 0, 0,
        ];

        let mult = MULT[(self.tran_speed >> 3) as usize];
        let unit = UNIT[(self.tran_speed & 0x7) as usize];

        if mult == 0 || unit == 0 {
            None
        } else {
            Some(mult * unit / 10)
        }
    }
}

impl std::fmt::Display for EmmcCsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CSD (RAW: 0x{:032X})", self.raw)?;
        writeln!(f, "CSD_STRUCTURE : {}", self.csd_structure)?;
        writeln!(f, "SPEC_VERS     : {}", self.spec_vers)?;
        writeln!(f, "CCC           : 0x{:03X}", self.ccc)?;
        writeln!(f, "TRAN_SPEED    : 0x{:02X}", self.tran_speed)?;

        if let Some(hz) = self.tran_speed_hz() {
            writeln!(f, "MAX_SPEED     : {} Hz", hz)?;
        }

        writeln!(
            f,
            "READ_BL_LEN   : {} ({} bytes)",
            self.read_bl_len,
            self.read_block_size()
        )?;

        writeln!(
            f,
            "WRITE_BL_LEN   : {} ({} bytes)",
            self.write_bl_len,
            self.write_block_size()
        )?;

        Ok(())
    }
}

// EXT_CSD field offsets
// Capacity / geometry
pub const EXT_CSD_DATA_SECTOR_SIZE: usize       =  61;
pub const EXT_CSD_USE_NATIVE_SECTOR: usize      =  62;
pub const EXT_CSD_NATIVE_SECTOR_SIZE: usize     =  63;
pub const EXT_CSD_SEC_COUNT: usize              = 212; // [215:212] Sector count (LE u32)

// Speed / bus configuration
pub const EXT_CSD_BUS_WIDTH: usize              = 183;
pub const EXT_CSD_HS_TIMING: usize              = 185;
pub const EXT_CSD_POWER_CLASS: usize            = 187;
pub const EXT_CSD_CMD_SET_REV: usize            = 189;
pub const EXT_CSD_CMD_SET: usize                = 191;
pub const EXT_CSD_REV: usize                    = 192;
pub const EXT_CSD_CSD_STRUCTURE: usize          = 194;
pub const EXT_CSD_DEVICE_TYPE: usize            = 196;
pub const EXT_CSD_DRIVER_STRENGTH: usize        = 197;

// Partitioning / boot
pub const EXT_CSD_SEC_FEATURE_SUPPORT: usize    = 231;
pub const EXT_CSD_BOOT_SIZE_MULT: usize         = 226;
pub const EXT_CSD_HC_ERASE_GRP_SIZE: usize      = 224;
pub const EXT_CSD_HC_WP_GRP_SIZE: usize         = 221;

pub const EXT_CSD_RPMB_SIZE_MULT: usize         = 168;
pub const EXT_CSD_PARTITION_CONFIG: usize       = 179;
pub const EXT_CSD_BOOT_BUS_CONDITIONS: usize    = 177;
pub const EXT_CSD_ERASE_GROUP_DEF: usize        = 175;
pub const EXT_CSD_BOOT_WP: usize                = 173;
pub const EXT_CSD_RST_N_FUNCTION: usize         = 162;
pub const EXT_CSD_PARTITIONING_SUPPORT: usize   = 160;
pub const EXT_CSD_PARTITIONS_ATTRIBUTE: usize   = 156;
pub const EXT_CSD_GP_SIZE_MULT: usize           = 143;

// Cache
pub const EXT_CSD_CACHE_SIZE: usize             = 249; // [252:249] LE u32
pub const EXT_CSD_CACHE_CTRL: usize             = 33;

// Command queue (eMMC 5.x)
pub const EXT_CSD_CMDQ_MODE_EN: usize           = 15;
pub const EXT_CSD_CMDQ_DEPTH: usize             = 307;

// Lifetime / health
pub const EXT_CSD_PRE_EOL_INFO: usize           = 267;
pub const EXT_CSD_DEVICE_LIFE_TIME_EST_TYP_A: usize = 268;
pub const EXT_CSD_DEVICE_LIFE_TIME_EST_TYP_B: usize = 269;

#[derive(Debug, Clone)]
pub struct ExtCsd {
    pub revision: u8,
    pub device_type: u8,
    pub bus_width: u8,
    pub hs_timing: u8,

    pub sec_count: u32,
    pub data_sector_size: u8,
    pub use_native_sector: u8,
    pub native_sector_size: u8,

    pub boot_size_mult: u8,
    pub rpmb_size_mult: u8,

    pub partition_config: u8,

    pub cache_size: u32,
    pub cache_ctrl: u8,

    pub pre_eol_info: u8,
    pub life_time_a: u8,
    pub life_time_b: u8,

    pub raw: [u8; 512],
}

impl ExtCsd {
    pub fn from_bytes(raw: [u8; 512]) -> Self {
        Self {
            revision: raw[EXT_CSD_REV],
            device_type: raw[EXT_CSD_DEVICE_TYPE],
            bus_width: raw[EXT_CSD_BUS_WIDTH],
            hs_timing: raw[EXT_CSD_HS_TIMING],

            sec_count: u32::from_le_bytes(
                raw[EXT_CSD_SEC_COUNT..EXT_CSD_SEC_COUNT + 4]
                    .try_into()
                    .unwrap()
            ),
            data_sector_size: raw[EXT_CSD_DATA_SECTOR_SIZE],
            use_native_sector: raw[EXT_CSD_USE_NATIVE_SECTOR],
            native_sector_size: raw[EXT_CSD_NATIVE_SECTOR_SIZE],

            boot_size_mult: raw[EXT_CSD_BOOT_SIZE_MULT],
            rpmb_size_mult: raw[EXT_CSD_RPMB_SIZE_MULT],

            partition_config: raw[EXT_CSD_PARTITION_CONFIG],

            cache_size: u32::from_le_bytes(
                raw[EXT_CSD_CACHE_SIZE..EXT_CSD_CACHE_SIZE + 4]
                    .try_into()
                    .unwrap(),
            ),

            cache_ctrl: raw[EXT_CSD_CACHE_CTRL],

            pre_eol_info: raw[EXT_CSD_PRE_EOL_INFO],
            life_time_a: raw[EXT_CSD_DEVICE_LIFE_TIME_EST_TYP_A],
            life_time_b: raw[EXT_CSD_DEVICE_LIFE_TIME_EST_TYP_B],

            raw,
        }
    }

    pub fn sector_count(&self) -> u32 {
        self.sec_count
    }
    
    pub fn capacity_bytes(&self) -> u64 {
        self.sec_count as u64 * 512
    }

    pub fn capacity_gib(&self) -> f64 {
        self.capacity_bytes() as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    pub fn boot_partition_size_bytes(&self) -> u64 {
        self.boot_size_mult as u64 * 128 * 1024
    }

    pub fn rpmb_size_bytes(&self) -> u64 {
        self.rpmb_size_mult as u64 * 128 * 1024
    }

    pub fn revision_name(&self) -> &'static str {
        match self.revision {
            0 => "obsolete",
            1 => "MMC 4.0",
            2 => "MMC 4.1",
            3 => "MMC 4.2",
            5 => "MMC 4.41",
            6 => "MMC 4.5",
            7 => "MMC 5.0",
            8 => "MMC 5.1",
            _ => "unknown",
        }
    }

    pub fn hs_timing_name(&self) -> &'static str {
        match self.hs_timing {
            0 => "Legacy",
            1 => "High Speed",
            2 => "HS200",
            3 => "HS400",
            _ => "Unknown",
        }
    }

    pub fn device_type_strings(&self) -> Vec<&'static str> {
        let mut v = Vec::new();

        let dt = self.device_type;

        if dt & (1 << 0) != 0 {
            v.push("HS 26 MHz");
        }

        if dt & (1 << 1) != 0 {
            v.push("HS 52 MHz");
        }

        if dt & (1 << 2) != 0 {
            v.push("DDR52 @1.8V/3V");
        }

        if dt & (1 << 3) != 0 {
            v.push("DDR52 @1.2V");
        }

        if dt & (1 << 4) != 0 {
            v.push("HS200 @1.8V");
        }

        if dt & (1 << 5) != 0 {
            v.push("HS200 @1.2V");
        }

        if dt & (1 << 6) != 0 {
            v.push("HS400 @1.8V");
        }

        if dt & (1 << 7) != 0 {
            v.push("HS400 @1.2V");
        }

        v
    }
}

impl fmt::Display for ExtCsd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "EXT_CSD Revision  : {}", self.revision_name())?;
        writeln!(
            f,
            "Capacity          : {:.2} GiB",
            self.capacity_gib()
        )?;
        writeln!(
            f,
            "Sector Count      : {}",
            self.sec_count
        )?;
        writeln!(
            f,
            "Data Sector Size  : {}",
            self.data_sector_size
        )?;
        writeln!(
            f,
            "Use native Sector : {}",
            self.use_native_sector
        )?;
        writeln!(
            f,
            "Native Sector Size: {}",
            self.native_sector_size
        )?;
        writeln!(
            f,
            "Boot Partition    : {} KiB",
            self.boot_partition_size_bytes() / 1024
        )?;
        writeln!(
            f,
            "RPMB Size         : {} KiB",
            self.rpmb_size_bytes() / 1024
        )?;
        writeln!(
            f,
            "Timing Mode       : {}",
            self.hs_timing_name()
        )?;
        writeln!(
            f,
            "Bus Width         : 0x{:02X}",
            self.bus_width
        )?;
        writeln!(
            f,
            "Partition Config  : 0x{:02X}",
            self.partition_config
        )?;
        writeln!(
            f,
            "Cache Size        : {} KiB",
            self.cache_size / 1024
        )?;
        writeln!(
            f,
            "Cache Enabled     : {}",
            self.cache_ctrl != 0
        )?;
        writeln!(
            f,
            "Device Type       : {}",
            self.device_type_strings().join(", ")
        )?;
        writeln!(
            f,
            "PRE_EOL_INFO      : 0x{:02X}",
            self.pre_eol_info
        )?;
        writeln!(
            f,
            "LIFE_TIME_A       : 0x{:02X}",
            self.life_time_a
        )?;
        writeln!(
            f,
            "LIFE_TIME_B       : 0x{:02X}",
            self.life_time_b
        )
    }
}

/// eMMC card info assembled from CID + CSD + EXT_CSD.
#[derive(Debug, Clone)]
pub struct MmcInfo {
    pub cid: EmmcCid,
    pub csd: EmmcCsd,
    pub ext_csd: ExtCsd,
}

impl MmcInfo {
    pub fn card_id(&self) -> String {
        format!("{:?} ({:#02x}) {} ({}) FW:{}.{} SN:{:#08x}",
            self.cid.manufacturer, self.cid.mid, self.cid.pnm,
            self.cid.cbx, self.cid.hw_revision(), self.cid.fw_revision(), self.cid.psn
        )
    }
    
    pub fn sector_count(&self) -> u32 {
        self.ext_csd.sector_count()
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.sector_count() as u64 * 512
    }

    pub fn capacity_mb(&self) -> u64 {
        self.capacity_bytes() / (1024 * 1024)
    }

    pub fn boot_area_bytes(&self) -> u64 {
        self.ext_csd.boot_partition_size_bytes()
    }

    pub fn rpmb_bytes(&self) -> u64 {
        self.ext_csd.rpmb_size_bytes()
    }

    pub fn bus_width(&self) -> u8 {
        self.ext_csd.bus_width
    }

    pub fn hs_timing(&self) -> u8 {
        self.ext_csd.hs_timing
    }

    pub fn device_type(&self) -> u8 {
        self.ext_csd.device_type
    }
}

impl std::fmt::Display for MmcInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "CID: {}", self.card_id())?;
        writeln!(f, "{}", self.cid)?;
        writeln!(f, "{}", self.csd)?;
        writeln!(f, "{}", self.ext_csd)
    }
}

/// SPI Error flags (from status registers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorFlags(u32);

bitflags::bitflags! {
    impl ErrorFlags: u32 {
        const ERASE_RESET       = 1 << 0x0D;
        const ERROR             = 1 << 0x13;
        const CC_ERROR          = 1 << 0x14;
        const DEVICE_ECC_FAILED = 1 << 0x15;
        const ILLEGAL_COMMAND   = 1 << 0x16;
        const CRC_ERROR         = 1 << 0x17;
        const DEVICE_IS_LOCKED  = 1 << 0x19;
        const BLOCK_LENGTH_ERROR= 1 << 0x1D;
        const ADDRESS_MISALIGN  = 1 << 0x1E;
        const SEQ_ERROR         = 1 << 0x02;
    }
}

impl ErrorFlags {
    /// Check if any error flag is set
    pub fn has_error(&self) -> bool {
        !self.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmcPresentState(pub u32);

impl MmcPresentState {
    pub fn app_cmd(&self) -> bool {
        (self.0 & (1 << 4)) != 0
    }

    pub fn is_ready_for_data(&self) -> bool {
        (self.0 & (1 << 7)) != 0
    }

    pub fn status(&self) -> Option<MmcState> {
        let status_bits = (self.0 & 0x1E00) >> 9;
        MmcState::from_bits(status_bits as u8)
    }

    pub fn error_flags(&self) -> Option<ErrorFlags> {
        return ErrorFlags::from_bits(self.0);
    }
}

impl fmt::Display for MmcPresentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MmcPresentState")?;
        write!(f, "  AppCmd: {}", self.app_cmd())?;
        write!(f, "  ReadyForData: {}", self.is_ready_for_data())?;
        write!(f, "  Status: {:?}", self.status())?;
        write!(f, "  Error flags: {:?}", self.error_flags())
    }
}

// Interrupts

/// Status bits (Interrupt status)
pub mod status {
    /// Command/Busy status - written to initiate operations
    pub const CMD_COMPLETE: u8 = 0;

    /// Transfer complete status - indicates block transfer is finished
    pub const TRANSFER_COMPLETE: u8 = 1;

    /// Data ready status - indicates 512 bytes can be written to DataFifo
    pub const DATA_WRITE_READY: u8 = 4;
    
    /// Data ready status - indicates 512 bytes are ready to read from DataFifo
    pub const DATA_READ_READY: u8 = 5;
}

pub const INTERRUPT_STATUS_EN: u32 = 0x1FFF_0033;
pub const INTERRUPT_SIGNAL_EN: u32 = 0x1FFF_0033;
pub const CLEAR_INTERRUPTS: u32 = 0xFFFF_FFFF;
/// Interrupt Error flag
pub const ERROR_INTERRUPT: u32 = 1 << 15;

// Clock
pub const CLK_SW_RESET_CMD_DAT: u32 = 0x0600_0000;

// Timings
pub const TIMING_HS: u32 = 0x03B9_0100;
pub const TIMING_HS200: u32 = 0x03B9_0200;
pub const TIMING_LEGACY: u32 = 0x03B9_0000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transferop_bits() {
        assert_eq!(TransferOp::Read.bits(), 0x1);
        assert_eq!(TransferOp::Write.bits(), 0x2);
        assert_eq!(TransferOp::bit_length(), 2);
    }

    #[test]
    fn test_register_addresses() {
        assert_eq!(Register::Argument.address(), 0x02);
        assert_eq!(Register::InitCommand.address(), 0x44);
        assert_eq!(Register::bit_length(), 8);
    }

    #[test]
    fn test_register_from_address() {
        assert_eq!(Register::from_address(0x02), Some(Register::Argument));
        assert_eq!(Register::from_address(0x44), Some(Register::InitCommand));
        assert_eq!(Register::from_address(0xFF), None);
    }

    #[test]
    fn test_data_sizes() {
        assert_eq!(DataSize::Register.bytes(), 4);
        assert_eq!(DataSize::Page.bytes(), 512);
    }
}
