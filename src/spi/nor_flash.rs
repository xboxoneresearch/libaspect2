//! JEDEC SPI NOR flash driver
//!
//! Implements standard SPI NOR flash operations over a `RawSpiBackend`.
//! Parallel to `EmmcReader` (which drives the custom eMMC bridge IC over the same
//! SPI bus), but speaks directly to a NOR flash chip without any intermediate
//! controller framing.

use super::backend::RawSpiBackend;
use super::protocol::nor::{JedecId, NorOpcode, status};
use crate::error::Error;
use crate::prelude::*;
use crate::spi::backend::GpioControl;

/// JEDEC SPI NOR flash driver
///
/// # Type parameters
/// * `B` — a [`RawSpiBackend`] that owns the SPI bus
/// * `C` — a clock/delay source (see [`crate::clock`])
pub struct NorFlash<B: RawSpiBackend + GpioControl, C: ClockTrait + DelayNs + Clone> {
    pub backend: B,
    clock: C,
    jedec_id: Option<JedecId>,
}

impl<B: RawSpiBackend + GpioControl, C: ClockTrait + DelayNs + Clone> NorFlash<B, C> {
    pub fn new(backend: B, clock: C) -> Self {
        Self {
            backend,
            clock,
            jedec_id: None,
        }
    }

    /// Cached JEDEC ID from the last successful [`init`](Self::init) call.
    pub fn jedec_id(&self) -> Option<JedecId> {
        self.jedec_id
    }

    /// Initialize: set up the backend and read the JEDEC ID.
    ///
    /// Returns an error if the device does not respond with a valid ID
    /// (all-0xFF means the bus is floating; 0x00 means no device).
    pub fn init(&mut self) -> Result<JedecId, Error> {
        self.backend.initialize()?;
        let id = self.read_jedec_id()?;
        if !id.is_valid() {
            return Err(Error::NorInvalidJedecId(id));
        }
        self.jedec_id = Some(id);
        Ok(id)
    }

    // -----------------------------------------------------------------------
    // Public API: SMC Reset
    // -----------------------------------------------------------------------

    pub fn assert_reset(&mut self) -> Result<(), Error> {
        self.backend.set_reset(true)
    }

    pub fn release_reset(&mut self) -> Result<(), Error> {
        self.backend.set_reset(false)
    }
    
    // -----------------------------------------------------------------------
    // Public API: read
    // -----------------------------------------------------------------------

    /// Read `buf.len()` bytes starting at 24-bit `addr`.
    pub fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Error> {
        self.backend.spi_transaction(&addr_cmd(NorOpcode::Read, addr), &[], buf)
    }

    /// Fast-read with one dummy byte after the address (allows higher SPI clock).
    pub fn fast_read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), Error> {
        // [opcode, A23..A16, A15..A8, A7..A0, dummy]
        let cmd = [
            NorOpcode::FastRead as u8,
            (addr >> 16) as u8,
            (addr >> 8) as u8,
            addr as u8,
            0x00,
        ];
        self.backend.spi_transaction(&cmd, &[], buf)
    }

    // -----------------------------------------------------------------------
    // Public API: write / erase
    // -----------------------------------------------------------------------

    /// Program up to one page (typically 256 bytes) starting at `addr`.
    ///
    /// Caller is responsible for ensuring `addr` is page-aligned and that
    /// `data` does not cross a page boundary.
    pub fn page_program(&mut self, addr: u32, data: &[u8]) -> Result<(), Error> {
        self.write_enable()?;
        self.backend
            .spi_transaction(&addr_cmd(NorOpcode::PageProgram, addr), data, &mut [])?;
        self.wait_ready(5_000)
    }

    /// Erase the 4 KiB sector that contains `addr`.
    pub fn sector_erase_4k(&mut self, addr: u32) -> Result<(), Error> {
        self.write_enable()?;
        self.backend
            .spi_transaction(&addr_cmd(NorOpcode::SectorErase4K, addr), &[], &mut [])?;
        self.wait_ready(10_000)
    }

    /// Erase the 32 KiB block that contains `addr`.
    pub fn block_erase_32k(&mut self, addr: u32) -> Result<(), Error> {
        self.write_enable()?;
        self.backend
            .spi_transaction(&addr_cmd(NorOpcode::BlockErase32K, addr), &[], &mut [])?;
        self.wait_ready(30_000)
    }

    /// Erase the 64 KiB block that contains `addr`.
    pub fn block_erase_64k(&mut self, addr: u32) -> Result<(), Error> {
        self.write_enable()?;
        self.backend
            .spi_transaction(&addr_cmd(NorOpcode::BlockErase64K, addr), &[], &mut [])?;
        self.wait_ready(30_000)
    }

    /// Erase the entire chip.  May take seconds to minutes depending on capacity.
    pub fn chip_erase(&mut self) -> Result<(), Error> {
        self.write_enable()?;
        self.backend
            .spi_transaction(&[NorOpcode::ChipErase as u8], &[], &mut [])?;
        self.wait_ready(300_000)
    }

    // -----------------------------------------------------------------------
    // Public API: power / reset
    // -----------------------------------------------------------------------

    /// Issue an in-band software reset (RSTEN + RST, two separate transactions).
    pub fn software_reset(&mut self) -> Result<(), Error> {
        self.backend
            .spi_transaction(&[NorOpcode::EnableReset as u8], &[], &mut [])?;
        self.backend
            .spi_transaction(&[NorOpcode::Reset as u8], &[], &mut [])
    }

    /// Put the device into deep power-down mode.
    pub fn deep_power_down(&mut self) -> Result<(), Error> {
        self.backend
            .spi_transaction(&[NorOpcode::DeepPowerDown as u8], &[], &mut [])
    }

    /// Wake the device from deep power-down.
    pub fn release_from_dpd(&mut self) -> Result<(), Error> {
        self.backend
            .spi_transaction(&[NorOpcode::ReleaseDpd as u8], &[], &mut [])
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn read_jedec_id(&mut self) -> Result<JedecId, Error> {
        let mut buf = [0u8; 3];
        self.backend
            .spi_transaction(&[NorOpcode::ReadJedecId as u8], &[], &mut buf)?;
        Ok(JedecId {
            manufacturer: buf[0],
            device: [buf[1], buf[2]],
        })
    }

    fn read_status(&mut self) -> Result<u8, Error> {
        let mut buf = [0u8; 1];
        self.backend
            .spi_transaction(&[NorOpcode::ReadStatus1 as u8], &[], &mut buf)?;
        Ok(buf[0])
    }

    fn write_enable(&mut self) -> Result<(), Error> {
        self.backend
            .spi_transaction(&[NorOpcode::WriteEnable as u8], &[], &mut [])
    }

    /// Poll the BUSY bit until the device is idle or `timeout_ms` elapses.
    fn wait_ready(&mut self, timeout_ms: u32) -> Result<(), Error> {
        let clock_clone = self.clock.clone();
        let mut t = Timer::new(&clock_clone);
        t.start(Duration::from_millis(timeout_ms.into()));

        loop {
            let s = self.read_status()?;
            if s & status::BUSY == 0 {
                return Ok(());
            }
            if t.is_expired().unwrap_or(false) {
                return Err(Error::Timeout);
            }
        }
    }
}

/// Build a 4-byte [opcode, A23, A15, A7] command word.
fn addr_cmd(opcode: NorOpcode, addr: u32) -> [u8; 4] {
    [
        opcode as u8,
        (addr >> 16) as u8,
        (addr >> 8) as u8,
        addr as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ClockTrait;

    #[derive(Clone)]
    struct MockClock;

    impl ClockTrait for MockClock {
        type Instant = std::time::Instant;
        fn now(&self) -> Self::Instant {
            std::time::Instant::now()
        }
    }

    impl DelayNs for MockClock {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    struct MockNorBackend {
        jedec: [u8; 3],
        status: u8,
        _mem: Vec<u8>,
        last_write: Vec<u8>,
    }

    impl MockNorBackend {
        fn new(jedec: [u8; 3]) -> Self {
            Self {
                jedec,
                status: 0,
                _mem: vec![0xFF; 256],
                last_write: Vec::new(),
            }
        }
    }

    impl RawSpiBackend for MockNorBackend {
        fn spi_transaction(
            &mut self,
            cmd: &[u8],
            write: &[u8],
            read: &mut [u8],
        ) -> Result<(), Error> {
            if cmd.is_empty() {
                return Ok(());
            }
            match cmd[0] {
                0x9F => {
                    // RDID
                    read.copy_from_slice(&self.jedec[..read.len()]);
                }
                0x05 => {
                    // RDSR
                    if !read.is_empty() {
                        read[0] = self.status;
                    }
                }
                0x06 => {} // WREN
                0x02 => {
                    // PP
                    self.last_write = write.to_vec();
                    self.status = 0; // already idle in mock
                }
                _ => {}
            }
            Ok(())
        }

        fn set_clock_freq(&mut self, _freq_khz: u32) -> Result<(), Error> {
            Ok(())
        }

        fn initialize(&mut self) -> Result<(), Error> {
            Ok(())
        }

        fn reset(&mut self) -> Result<(), Error> {
            Ok(())
        }
    }

    impl GpioControl for MockNorBackend {
        fn set_chip_select(&mut self, _asserted: bool) -> Result<(), Error> {
            Ok(())
        }
    
        fn set_reset(&mut self, _asserted: bool) -> Result<(), Error> {
            Ok(())
        }
    
        fn set_enable(&mut self, _enabled: bool) -> Result<(), Error> {
            Ok(())
        }
    }
    
    #[test]
    fn test_init_valid_jedec() {
        let backend = MockNorBackend::new([0xEF, 0x40, 0x18]); // Winbond W25Q128
        let mut flash = NorFlash::new(backend, MockClock);
        let id = flash.init().unwrap();
        assert_eq!(id.manufacturer, 0xEF);
        assert_eq!(id.device, [0x40, 0x18]);
    }

    #[test]
    fn test_init_floating_bus() {
        let backend = MockNorBackend::new([0xFF, 0xFF, 0xFF]);
        let mut flash = NorFlash::new(backend, MockClock);
        assert!(matches!(flash.init(), Err(Error::NorInvalidJedecId(_))));
    }

    #[test]
    fn test_addr_cmd() {
        assert_eq!(
            addr_cmd(NorOpcode::Read, 0x12_3456),
            [0x03, 0x12, 0x34, 0x56]
        );
    }

    #[test]
    fn test_page_program_sends_data() {
        let backend = MockNorBackend::new([0xEF, 0x40, 0x18]);
        let mut flash = NorFlash::new(backend, MockClock);
        flash.init().unwrap();
        let data = [0xDE, 0xAD, 0xBE, 0xEF];
        flash.page_program(0x001000, &data).unwrap();
        assert_eq!(flash.backend.last_write, data);
    }
}
