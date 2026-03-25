//! Arasan SDHCI eMMC Controller over SPI
//!
//! Full MMC initialization and data transfer using the SpiBackend transport.
//! Register indices correspond directly to SpiBackend register addresses.

use super::backend::SpiBackend;
use super::protocol::commands::{Register, status, transfer_config};
use crate::error::Error;
use crate::prelude::*;
use crate::spi::protocol::commands::ERROR_INTERRUPT;

// ---------------------------------------------------------------------------
// SMC fuse hashes (Xbox debug probe)
// ---------------------------------------------------------------------------

const B1SMCBL_HASH_DEVKIT: [u8; 16] = hex_literal::hex!("C0DE15B90000FFFFA5A55A5A1234FEDC");
const B1SMCBL_HASH_RTL_A: [u8; 16] = hex_literal::hex!("2C0278DBD3716D1996C5E5A4560B3F6A");
const B1SMCBL_HASH_RTL_B: [u8; 16] = hex_literal::hex!("40427E9153E88CA7B2BD3812FEB69B65");
const B1SMCBL_HASH_RTL_C: [u8; 16] = hex_literal::hex!("A3192969B3B3068F1246B9B4EF18E99E");
const B1SMCBL_HASH_RTL_D: [u8; 16] = hex_literal::hex!("DF219ABE760F9B32BCBE86C254010F52");

#[derive(Debug)]
#[allow(non_snake_case, dead_code)]
pub struct SmcFuses {
    ECID: [u8; 8],
    Exp1SMCBLDigest: [u8; 16],
    RsvdPublic: [u8; 8],
    RsvdPrivate: [u8; 8],
    ChipID: [u8; 12],
    SbRev: [u8; 4],
}

#[cfg(feature = "std")]
impl std::fmt::Display for SmcFuses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let smc_flavor = match self.Exp1SMCBLDigest {
            B1SMCBL_HASH_DEVKIT => "Development Mode, SMCFWKey:Devkit".to_string(),
            B1SMCBL_HASH_RTL_A => "Production Mode, SMCFWKey:rtlA".to_string(),
            B1SMCBL_HASH_RTL_B => "Production Mode, SMCFWKey:rtlB".to_string(),
            B1SMCBL_HASH_RTL_C => "Production Mode, SMCFWKey:rtlC".to_string(),
            B1SMCBL_HASH_RTL_D => "Production Mode, SMCFWKey:rtlD".to_string(),
            _ => format!("!UNKNOWN! ({})", hex::encode(self.Exp1SMCBLDigest)),
        };
        writeln!(f, "ECID: {}", hex::encode(self.ECID))?;
        writeln!(f, "Exp1SMCBLDigest: {smc_flavor}")?;
        writeln!(f, "RsvdPublic: {}", hex::encode(self.RsvdPublic))?;
        writeln!(f, "RsvdPrivate: {}", hex::encode(self.RsvdPrivate))?;
        writeln!(f, "ChipID: {}", hex::encode(self.ChipID))?;
        writeln!(f, "SB Rev: {}", hex::encode(self.SbRev))
    }
}

// ---------------------------------------------------------------------------
// MMC command encoding
//
// Packed u32: upper 16 = SDHCI Command Register, lower 16 = Transfer Mode.
//   Command Register: [13:8] index, [5] data-present, [4] index-check,
//                     [3] CRC-check, [1:0] response type
// ---------------------------------------------------------------------------

const fn make_cmd(index: u8, resp: u8) -> u32 {
    ((index as u32) << 24) | ((resp as u32) << 16)
}

const RESP_NONE: u8 = 0x00;
const RESP_R2: u8 = 0x09; // 136-bit
const RESP_R3: u8 = 0x02; // 48-bit, no CRC/Index
const RESP_R1: u8 = 0x1A; // 48-bit, CRC+Index
const RESP_R1B: u8 = 0x1B; // 48-bit, CRC+Index, busy

// Non-data commands
const CMD0: u32 = make_cmd(0, RESP_NONE); // GO_IDLE
const CMD1: u32 = make_cmd(1, RESP_R3); // SEND_OP_COND
const CMD2: u32 = make_cmd(2, RESP_R2); // ALL_SEND_CID
const CMD3: u32 = make_cmd(3, RESP_R1); // SET_RCA
const CMD6: u32 = make_cmd(6, RESP_R1B); // SWITCH
const CMD7_SEL: u32 = make_cmd(7, RESP_R1); // SELECT_CARD
const CMD7_DESEL: u32 = make_cmd(7, 0x18); // DESELECT_CARD
const CMD13: u32 = make_cmd(13, RESP_R1); // SEND_STATUS
const CMD16: u32 = make_cmd(16, RESP_R1); // SET_BLOCKLEN
const CMD35: u32 = make_cmd(35, RESP_R1); // ERASE_GROUP_START
const CMD36: u32 = make_cmd(36, RESP_R1); // ERASE_GROUP_END
const CMD38: u32 = make_cmd(38, RESP_R1); // ERASE

// Data transfer commands: CMD Register (upper 16) | Transfer Mode (lower 16)
//   Transfer Mode bits: [5] multi-block, [4] read-direction, [2] auto-CMD12,
//                       [1] block-count-enable
const CMD8_EXT_CSD: u32 = 0x083A_0010; // SEND_EXT_CSD (single read)
const CMD17_READ: u32 = 0x113A_0010; // READ_SINGLE_BLOCK
const CMD18_READ: u32 = 0x123A_0036; // READ_MULTIPLE_BLOCK
const CMD24_WRITE: u32 = 0x183A_0000; // WRITE_BLOCK
const CMD25_WRITE: u32 = 0x193A_0026; // WRITE_MULTIPLE_BLOCK

const RCA: u32 = 10;
const RCA_ARG: u32 = RCA << 16;
const BLOCK_SIZE: u32 = 512;
const BASE_CLOCK_MHZ: f64 = 196.875;

// ---------------------------------------------------------------------------
// Erase type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseType {
    Erase = 0,
    Trim = 1,
    Discard = 3,
}

// ---------------------------------------------------------------------------
// EmmcReader — the controller
// ---------------------------------------------------------------------------

pub struct EmmcReader<B: SpiBackend, C: ClockTrait + DelayNs + Clone> {
    pub backend: B,
    internal_clock: C,
    initialized: bool,
    block_size: u32,
    clock_mhz: f64,
    cid: [u32; 4],
}

impl<B: SpiBackend, C: ClockTrait + DelayNs + Clone> EmmcReader<B, C> {
    /// Create a new reader with the specified backend
    pub fn new(backend: B, clock_impl: C) -> Self {
        Self {
            backend,
            internal_clock: clock_impl,
            initialized: false,
            block_size: 0,
            clock_mhz: 0.0,
            cid: [0; 4],
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn cid(&self) -> &[u32; 4] {
        &self.cid
    }

    fn decode_response_r1x(&mut self) {}

    // -----------------------------------------------------------------------
    // Register helpers
    // -----------------------------------------------------------------------

    fn read_reg(&mut self, r: Register) -> Result<u32, Error> {
        self.backend.read_register(r)
    }

    fn write_reg(&mut self, r: Register, v: u32) -> Result<(), Error> {
        self.backend.write_register(r, v)
    }

    fn modify_reg(&mut self, r: Register, set: u32, clear: u32) -> Result<(), Error> {
        let v = self.read_reg(r)?;
        self.write_reg(r, (v & !clear) | set)
    }

    // -----------------------------------------------------------------------
    // MMC command interface
    // -----------------------------------------------------------------------

    /// Issue a non-data MMC command and wait for completion.
    fn command(&mut self, cmd_word: u32, argument: u32) -> Result<(), Error> {
        self.write_reg(Register::Argument, argument)?;
        self.write_reg(Register::CommandAndTransferMode, cmd_word)?;

        // Wait for Command Complete (bit 0)
        self.poll_bit(Register::InterruptStatus, 0, true, true, None)?;

        // Handle response type (bits [17:16] of packed word)
        match (cmd_word >> 16) & 3 {
            0..=2 => {} // none / R2 / R1
            3 => {
                // R1b — also wait for Transfer Complete (bit 1)
                let _ = self.poll_bit(Register::InterruptStatus, 1, true, true, Some(5000));
            }
            _ => return Err(Error::RegisterAccessFailed),
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Status polling
    // -----------------------------------------------------------------------

    /// Poll until `(reg_value & mask) == expected`. Returns error on timeout
    /// or if the error-interrupt bit is set (when polling INT_STATUS).
    fn poll_mask(
        &mut self,
        register: Register,
        mask: u32,
        expected: u32,
        clear: bool,
        timeout: Option<u32>,
    ) -> Result<(), Error> {
        let clock_clone = self.internal_clock.clone();
        let mut t = Timer::new(&clock_clone);
        let maybe_timer = timeout.map(|val| {
            t.start(Duration::from_millis(val.into()));
            t
        });

        loop {
            let val = self.read_reg(register)?;

            if register == Register::InterruptStatus && val & ERROR_INTERRUPT != 0 {
                // Clear the error so we don't loop on it
                let _ = self.write_reg(Register::InterruptStatus, val);
                return Err(Error::MmcHardwareError { status: val });
            }
            if val & mask == expected & mask {
                if clear {
                    self.write_reg(register, mask)?;
                }
                return Ok(());
            }
            if let Some(timer) = &maybe_timer
                && timer.is_expired().unwrap()
            {
                return Err(Error::Timeout);
            }
        }
    }

    fn poll_bit(
        &mut self,
        register: Register,
        bit: u8,
        set: bool,
        clear: bool,
        timeout: Option<u32>,
    ) -> Result<(), Error> {
        let mask = 1u32 << bit;
        self.poll_mask(register, mask, if set { mask } else { 0 }, clear, timeout)
    }

    fn clear_interrupts(&mut self) -> Result<(), Error> {
        self.write_reg(Register::InterruptStatus, 0xFFFF_FFFF)
    }

    // -----------------------------------------------------------------------
    // Clock control
    // -----------------------------------------------------------------------

    /// Program the SDHCI clock divider for a target frequency.
    fn set_clock(&mut self, freq_mhz: f64) -> Result<(), Error> {
        // Enable internal clock (bit 0)
        self.modify_reg(Register::Command, 1, 0)?;

        // Wait for Internal Clock Stable (bit 1)
        self.poll_bit(Register::Command, 1, true, false, Some(1000))?;

        // Disable SD Clock Output (bit 2) while reprogramming
        self.modify_reg(Register::Command, 0, 4)?;

        // 10-bit divider: freq = base / (2 * divider)
        let divider = (BASE_CLOCK_MHZ / (2.0 * freq_mhz) + 0.5) as u16;
        let clk = self.read_reg(Register::Command)?;
        let low = (divider << 8) | (clk as u16 & 0x3F) | ((divider >> 2) & 0xC0);
        self.write_reg(Register::Command, (clk & 0xFFFF_0000) | low as u32)?;

        // Re-enable SD Clock Output (bit 2)
        self.modify_reg(Register::Command, 4, 0)?;
        self.clock_mhz = freq_mhz;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Interrupt enables
    // -----------------------------------------------------------------------

    fn enable_interrupts(&mut self) -> Result<(), Error> {
        // INT_STATUS_EN: enable Command Complete, Transfer Complete,
        // Buffer Write/Read Ready, and all error interrupts
        self.write_reg(Register::InterruptStatusEn, 0x1FFF_0033)?;
        self.write_reg(Register::InterruptSignalEn, 0x17FF_0033)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Card lifecycle
    // -----------------------------------------------------------------------

    /// CMD0 → CMD1 loop → CMD2 → CMD3: bring card from Idle to Standby.
    fn enter_standby(&mut self) -> Result<(), Error> {
        self.command(CMD0, 0)?;

        // Set data timeout counter (bits [19:17])
        self.modify_reg(Register::Command, 0x000E_0000, 0)?;

        // CMD1 loop — wait for card ready (bit 31 of OCR)
        loop {
            self.command(CMD1, 0x4000_0100)?;
            if self.read_reg(Register::Response0And1)? & 0x8000_0000 != 0 {
                break;
            }
        }

        // CMD2 — read CID
        self.command(CMD2, 0)?;
        self.cid = [
            self.read_reg(Register::Response0And1)?,
            self.read_reg(Register::Response2And3)?,
            self.read_reg(Register::Response4And5)?,
            self.read_reg(Register::Response6And7)?,
        ];

        // CMD3 — assign RCA
        self.command(CMD3, RCA_ARG)
    }

    fn select_card(&mut self, select: bool) -> Result<(), Error> {
        if select {
            self.command(CMD7_SEL, RCA_ARG)
        } else {
            self.command(CMD7_DESEL, 0)
        }
    }

    fn set_block_size(&mut self, size: u32) -> Result<(), Error> {
        self.command(CMD16, size & 0xFFF)?;
        self.block_size = size;
        Ok(())
    }

    fn set_block_count(&mut self, count: u16) -> Result<(), Error> {
        self.write_reg(
            Register::BlockSizeCount,
            ((count as u32) << 16) | (self.block_size & 0xFFF),
        )
    }

    fn send_status(&mut self) -> Result<(), Error> {
        self.command(CMD13, RCA_ARG)
    }

    fn set_xip_output_delay(&mut self, value: u32) -> Result<(), Error> {
        self.write_reg(Register::XipOutputDelay, value)
    }

    // -----------------------------------------------------------------------
    // High-speed / HS200 configuration
    // -----------------------------------------------------------------------

    fn configure_high_speed(&mut self, freq_mhz: f64) -> Result<(), Error> {
        if freq_mhz > 25.0 {
            // 1.8V signalling (bit 19 in Host Control 2)
            self.modify_reg(Register::AutoCmdHost2, 0x0008_0000, 0)?;
            // High-Speed Enable (bit 2 of Host Control)
            self.modify_reg(Register::HostControl, 4, 0)?;
            let xip = if freq_mhz > 52.0 { 0 } else { 0x0007_0001 };
            self.set_xip_output_delay(xip)?;
        }
        self.set_clock(freq_mhz)?;
        if freq_mhz > 52.0 {
            self.tuning()?;
        }
        Ok(())
    }

    /// Full card init: identification → select → switch timing → high-speed clock.
    fn mmc_init(&mut self, freq_mhz: f64) -> Result<(), Error> {
        self.clear_interrupts()?;

        self.enter_standby()?;
        self.select_card(true)?;

        // CMD6 SWITCH: HS_TIMING = 1 initially
        self.command(CMD6, 0x03B9_0100)?;

        // CMD6 SWITCH: BUS_WIDTH = 8-bit (EXT_CSD[183] = 2)
        // Must tell the card BEFORE switching host controller bus width
        self.command(CMD6, 0x03B7_0200)?;

        // 8-bit bus width on host side: clear bits [5:3], set bit 5
        let hc = (self.read_reg(Register::HostControl)? & !0x38) | 0x20;
        self.write_reg(Register::HostControl, hc)?;

        self.set_block_size(BLOCK_SIZE)?;

        let freq = if freq_mhz > 0.0 {
            freq_mhz
        } else {
            self.clock_mhz
        };
        self.clock_mhz = freq;

        // Select timing mode by target speed
        let timing = if freq > 52.0 {
            0x03B9_0200 // HS200
        } else if freq > 25.0 {
            0x03B9_0100 // High Speed
        } else {
            0x03B9_0000 // Legacy
        };
        self.command(CMD6, timing)?;

        self.configure_high_speed(freq)
    }

    // -----------------------------------------------------------------------
    // HS200 tuning
    // -----------------------------------------------------------------------

    fn tuning(&mut self) -> Result<(), Error> {
        let saved_bs = self.block_size;
        self.block_size = 128;

        self.write_reg(Register::VendorTuning, 32)?;
        // Execute Tuning (bit 22 in Host Control 2)
        self.modify_reg(Register::AutoCmdHost2, 0x0040_0000, 0)?;

        loop {
            let mut buf = [0u8; 128];
            self.backend.read_data(Register::DataFifo, &mut buf)?;

            if self.read_reg(Register::AutoCmdHost2)? & 0x0040_0000 == 0 {
                self.block_size = saved_bs;
                return Ok(());
            }
        }
    }

    // -----------------------------------------------------------------------
    // Extended CSD
    // -----------------------------------------------------------------------

    pub fn read_ext_csd(&mut self, buf: &mut [u8; 512]) -> Result<(), Error> {
        self.set_block_count(1)?;
        self.clear_interrupts()?;

        // CMD8 SEND_EXT_CSD (data read)
        self.write_reg(Register::Argument, 0)?;
        self.write_reg(Register::CommandAndTransferMode, CMD8_EXT_CSD)?;

        // Wait for Command Complete
        self.poll_bit(Register::InterruptStatus, 0, true, true, Some(1000))?;

        // Wait for Buffer Read Ready (bit 5)
        self.poll_bit(Register::InterruptStatus, 5, true, true, Some(1000))?;

        // Read 512 bytes
        buf.fill(0);
        self.backend.read_data(Register::DataFifo, buf)?;

        // Wait for Transfer Complete (bit 1)
        self.poll_bit(Register::InterruptStatus, 1, true, true, Some(1000))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Public API: initialization
    // -----------------------------------------------------------------------

    /// Full initialization: bridge setup → sanity check → card init.
    ///
    /// Initializes the card at ~50 MHz (high-speed mode). Use `init_at_freq`
    /// for a different target clock.
    pub fn init(&mut self) -> Result<(), Error> {
        self.init_at_freq(50.0)
    }

    /// Initialize at a specific target clock frequency (MHz).
    ///
    /// * `<= 25.0` — Legacy mode
    /// * `<= 52.0` — High-Speed mode
    /// * `> 52.0`  — HS200 mode (with tuning)
    pub fn init_at_freq(&mut self, freq_mhz: f64) -> Result<(), Error> {
        if self.initialized {
            return Ok(());
        }

        // Hardware init (GPIO, SPI, reset)
        self.backend.initialize()?;

        // Enable the SPI → SDHCI bridge
        self.backend
            .write_register(Register::InitCommand, 0x0000_0003)?;

        // Wait for the bridge to come up (retries with backoff)
        self.sanity_check()?;

        // Ramp up SPI bus clock now that the link is verified
        // FT2232H supports up to 30 MHz
        self.backend.set_spi_clock(30000)?;

        // Enable SDHCI interrupts so polling works
        self.enable_interrupts()?;

        // Start with a slow identification clock (~400 kHz)
        self.set_clock(0.4)?;

        // Run full MMC init: standby → select → switch → high-speed clock
        self.mmc_init(freq_mhz)?;

        self.initialized = true;
        Ok(())
    }

    /// Reinitialize the controller clock/bus without re-enumerating the card.
    /// Use this after the card is already selected but you want a new speed.
    pub fn controller_init(&mut self, freq_mhz: f64) -> Result<(), Error> {
        let hc = (self.read_reg(Register::HostControl)? & !0x38) | 0x20;
        self.write_reg(Register::HostControl, hc)?;
        self.set_block_size(BLOCK_SIZE)?;

        let freq = if freq_mhz > 0.0 {
            freq_mhz
        } else {
            self.clock_mhz
        };
        self.clock_mhz = freq;
        self.configure_high_speed(freq)
    }

    // -----------------------------------------------------------------------
    // Sanity check
    // -----------------------------------------------------------------------

    fn sanity_check(&mut self) -> Result<(), Error> {
        // The bridge may need time after reset/enable; retry with backoff.
        for attempt in 0..10 {
            if attempt > 0 {
                self.internal_clock.delay_ms(50 * attempt as u32);
            }
            match self.try_sanity_check() {
                Ok(()) => return Ok(()),
                Err(_) if attempt < 9 => continue,
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    fn try_sanity_check(&mut self) -> Result<(), Error> {
        for test_value in [0x1234_5678u32, 0xEDCB_A987, 0x1234_5678, 0xEDCB_A987] {
            self.backend
                .write_register(Register::Argument, test_value)?;
            let readback = self.backend.read_register(Register::Argument)?;
            if readback != test_value {
                return Err(Error::SanityCheckFailed {
                    expected: test_value,
                    actual: readback,
                });
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Public API: read / write pages
    // -----------------------------------------------------------------------

    /// Read a single 512-byte block at the given LBA.
    pub fn read_page(&mut self, lba: u32, buf: &mut [u8; 512]) -> Result<(), Error> {
        self.set_block_count(1)?;
        self.clear_interrupts()?;

        // CMD17 READ_SINGLE_BLOCK
        self.write_reg(Register::Argument, lba)?;
        self.write_reg(Register::CommandAndTransferMode, CMD17_READ)?;

        // Command Complete
        self.poll_bit(Register::InterruptStatus, 0, true, true, Some(1000))?;

        // Buffer Read Ready (bit 5)
        self.poll_bit(Register::InterruptStatus, 5, true, true, Some(1000))?;

        // Read 512 bytes from the data FIFO
        self.backend.read_data(Register::DataFifo, buf)?;

        // Transfer Complete (bit 1)
        self.poll_bit(Register::InterruptStatus, 1, true, true, Some(1000))?;

        Ok(())
    }

    /// Read multiple contiguous 512-byte blocks.
    ///
    /// `buf` must be at least `count * 512` bytes.
    pub fn read_pages(&mut self, start_lba: u32, buf: &mut [u8], count: u32) -> Result<(), Error> {
        if count == 0 {
            return Ok(());
        }
        if count == 1 {
            let page: &mut [u8; 512] = (&mut buf[..512]).try_into().unwrap();
            return self.read_page(start_lba, page);
        }

        self.set_block_count(count as u16)?;
        self.clear_interrupts()?;

        // CMD18 READ_MULTIPLE_BLOCK
        self.write_reg(Register::Argument, start_lba)?;
        self.write_reg(Register::CommandAndTransferMode, CMD18_READ)?;

        // Command Complete
        self.poll_bit(Register::InterruptStatus, 0, true, true, Some(1000))?;

        // PIO: one block at a time
        for i in 0..count as usize {
            // Buffer Read Ready (bit 5)
            self.poll_bit(Register::InterruptStatus, 5, true, true, None)?;

            let start = i * 512;
            self.backend
                .read_data(Register::DataFifo, &mut buf[start..start + 512])?;
        }

        // Transfer Complete (bit 1)
        self.poll_bit(Register::InterruptStatus, 1, true, true, Some(5000))?;

        Ok(())
    }

    /// Write a single 512-byte block at the given LBA.
    pub fn write_page(&mut self, lba: u32, buf: &[u8; 512]) -> Result<(), Error> {
        self.set_block_count(1)?;
        self.clear_interrupts()?;

        // CMD24 WRITE_BLOCK
        self.write_reg(Register::Argument, lba)?;
        self.write_reg(Register::CommandAndTransferMode, CMD24_WRITE)?;

        // Command Complete
        self.poll_bit(Register::InterruptStatus, 0, true, true, Some(1000))?;

        // Buffer Write Ready (bit 4)
        self.poll_bit(Register::InterruptStatus, 4, true, true, Some(1000))?;

        // Write 512 bytes to the data FIFO
        self.backend.write_data(Register::DataFifo, buf)?;

        // Transfer Complete (bit 1)
        self.poll_bit(Register::InterruptStatus, 1, true, true, Some(5000))?;

        Ok(())
    }

    /// Write multiple contiguous 512-byte blocks.
    ///
    /// `buf` must be at least `count * 512` bytes.
    pub fn write_pages(&mut self, start_lba: u32, buf: &[u8], count: u32) -> Result<(), Error> {
        if count == 0 {
            return Ok(());
        }
        if count == 1 {
            let page: &[u8; 512] = buf[..512].try_into().unwrap();
            return self.write_page(start_lba, page);
        }

        self.set_block_count(count as u16)?;
        self.clear_interrupts()?;

        // CMD25 WRITE_MULTIPLE_BLOCK
        self.write_reg(Register::Argument, start_lba)?;
        self.write_reg(Register::CommandAndTransferMode, CMD25_WRITE)?;

        // Command Complete
        self.poll_bit(Register::InterruptStatus, 0, true, true, Some(1000))?;

        // PIO: one block at a time
        for i in 0..count as usize {
            // Buffer Write Ready (bit 4)
            self.poll_bit(Register::InterruptStatus, 4, true, true, None)?;

            let start = i * 512;
            self.backend
                .write_data(Register::DataFifo, &buf[start..start + 512])?;
        }

        // Transfer Complete (bit 1)
        self.poll_bit(Register::InterruptStatus, 1, true, true, Some(5000))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Erase / Trim / Discard
    // -----------------------------------------------------------------------

    pub fn erase(&mut self, kind: EraseType, start_byte: u64, length: u64) -> Result<(), Error> {
        if start_byte & 0x1FF != 0 || length & 0x1FF != 0 {
            return Err(Error::RegisterAccessFailed);
        }
        if length == 0 {
            return Ok(());
        }

        let start_sec = (start_byte >> 9) as u32;
        let end_sec = start_sec + (length >> 9) as u32;

        self.set_block_size(BLOCK_SIZE)?;

        let mut erase_group: u32 = 1;

        if kind == EraseType::Erase {
            let mut ecsd = [0u8; 512];
            self.read_ext_csd(&mut ecsd)?;
            erase_group = (ecsd[224] as u32) << 10;
        } else if kind == EraseType::Trim {
            let mut ecsd = [0u8; 512];
            self.read_ext_csd(&mut ecsd)?;
            if ecsd[231] & 0x10 == 0 {
                return Err(Error::MmcNotSupported);
            }
        }

        if !start_sec.is_multiple_of(erase_group) || !end_sec.is_multiple_of(erase_group) {
            return Err(Error::RegisterAccessFailed);
        }

        if kind == EraseType::Erase {
            self.command(CMD6, 0x03B1_0000)?; // enhanced-erase attribute
        }

        self.command(CMD35, start_sec)?;

        let last = end_sec
            .checked_sub(erase_group)
            .ok_or(Error::RegisterAccessFailed)?;
        if start_sec > last {
            return Err(Error::RegisterAccessFailed);
        }

        self.command(CMD36, last)?;
        self.command(CMD38, kind as u32)?;
        self.send_status()
    }

    // -----------------------------------------------------------------------
    // Sanitize
    // -----------------------------------------------------------------------

    pub fn sanitize(&mut self) -> Result<(), Error> {
        let mut ecsd = [0u8; 512];
        self.read_ext_csd(&mut ecsd)?;
        if ecsd[231] & 0x40 == 0 {
            return Err(Error::MmcNotSupported);
        }
        self.command(CMD6, 0x03A5_FF00)?;
        self.send_status()
    }

    // -----------------------------------------------------------------------
    // Async abort
    // -----------------------------------------------------------------------

    pub fn async_abort(&mut self, is_read: bool) -> Result<(), Error> {
        self.clear_interrupts()?;
        let c = make_cmd(12, if is_read { RESP_R1 } else { RESP_R1B });
        self.command(c, 0)?;
        self.modify_reg(Register::Command, 0x0600_0000, 0)?;
        self.poll_mask(Register::Command, 0x0600_0000, 0, false, Some(5000))
    }

    // -----------------------------------------------------------------------
    // Fuse dump (Xbox debug probe specific)
    // -----------------------------------------------------------------------

    pub fn dump_fuses(&mut self) -> Result<SmcFuses, Error> {
        self.backend.initialize()?;
        self.backend
            .write_register(Register::InitCommand, 0x0000_0003)?;

        // Wait for bridge to come up
        self.sanity_check()?;

        let mut buf = [0u8; 0x38];
        let mut pos = 0;
        for reg in Register::XipDataFirst.address()..=Register::XipDataLast.address() {
            let value = self.backend.read_register(reg)?;
            buf[pos..pos + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
            pos += size_of::<u32>();
        }

        // Copy data from buf into SMC_FUSES struct
        let mut offset = 0;
        let ecid: [u8; 8] = buf[offset..offset + 8].try_into().unwrap();
        offset += 8;
        let exp1smcbldigest: [u8; 16] = buf[offset..offset + 16].try_into().unwrap();
        offset += 16;
        let rsvdpublic: [u8; 8] = buf[offset..offset + 8].try_into().unwrap();
        offset += 8;
        let rsvdprivate: [u8; 8] = buf[offset..offset + 8].try_into().unwrap();
        offset += 8;
        let chipid: [u8; 12] = buf[offset..offset + 12].try_into().unwrap();
        offset += 12;
        let sbrev: [u8; 4] = buf[offset..offset + 4].try_into().unwrap();

        let fuses = SmcFuses {
            ECID: ecid,
            Exp1SMCBLDigest: exp1smcbldigest,
            RsvdPublic: rsvdpublic,
            RsvdPrivate: rsvdprivate,
            ChipID: chipid,
            SbRev: sbrev,
        };

        Ok(fuses)
    }

    // -----------------------------------------------------------------------
    // Debug
    // -----------------------------------------------------------------------

    /// Print all standard SDHCI registers.
    #[cfg(feature = "std")]
    pub fn dump_registers(&mut self) {
        for i in 0u8..=0x0F {
            if let Some(reg) = Register::from_address(i) {
                if let Ok(v) = self.read_reg(reg) {
                    println!("reg[0x{i:02X}] = 0x{v:08X}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock delay
    #[derive(Clone)]
    struct MockClock;

    impl DelayNs for MockClock {
        fn delay_ns(&mut self, ns: u32) {}
    }

    impl ClockTrait for MockClock {
        type Instant = std::time::Instant;

        fn now(&self) -> Self::Instant {
            std::time::Instant::now()
        }
    }

    struct MockBackend {
        registers: std::collections::HashMap<u8, u32>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                registers: std::collections::HashMap::new(),
            }
        }
    }

    impl SpiBackend for MockBackend {
        fn write_register<T: Into<u8>>(&mut self, register: T, data: u32) -> Result<(), Error> {
            self.registers.insert(register.into(), data);
            Ok(())
        }

        fn read_register<T: Into<u8>>(&mut self, register: T) -> Result<u32, Error> {
            Ok(*self.registers.get(&register.into()).unwrap_or(&0))
        }

        fn read_data<T: Into<u8>>(&mut self, _register: T, buffer: &mut [u8]) -> Result<(), Error> {
            buffer.fill(0);
            Ok(())
        }

        fn reset(&mut self) -> Result<(), Error> {
            Ok(())
        }

        fn initialize(&mut self) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn test_read_write_register() {
        let backend = MockBackend::new();
        let mut reader = EmmcReader::new(backend, MockClock);

        reader.write_reg(Register::Argument, 0xDEAD_BEEF).unwrap();
        let value = reader.read_reg(Register::Argument).unwrap();
        assert_eq!(value, 0xDEAD_BEEF);
    }

    #[test]
    fn test_sanity_check() {
        let backend = MockBackend::new();
        let mut reader = EmmcReader::new(backend, MockClock);
        reader.sanity_check().unwrap();
    }
}
