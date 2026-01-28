/// High-level eMMC SPI Reader
/// 
/// This module provides a clean, high-level API for reading from the eMMC chip,
/// using the backend abstraction to work with any SPI implementation.

use crate::error::Error;
use super::protocol::commands::{Register, status, transfer_config};
use super::backend::SpiBackend;

/// eMMC SPI Reader - works with any backend
pub struct EmmcReader<B: SpiBackend> {
    backend: B,
    initialized: bool,
}

impl<B: SpiBackend> EmmcReader<B> {
    /// Create a new reader with the specified backend
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            initialized: false,
        }
    }
    
    /// Send init sequence
    /// 
    /// To be ran after sanity check
    fn init_sequence(&mut self) -> Result<(), Error> {
        let res = self.read_register(Register::Command)?;
        assert_eq!(0x0, res);
        self.write_register(Register::Command, 0x1)?;
        let res = self.read_register(Register::Command)?;
        assert_eq!(0x3, res);
        let res = self.read_register(Register::Command)?;
        assert_eq!(0x3, res);

        self.write_register(Register::Command, 0x3)?;
        self.write_register(Register::Command, 0x43)?;
        self.write_register(Register::Command, 0x47)?;
        let res = self.read_register(Register::Config1)?;
        assert_eq!(0x0, res);

        self.write_register(Register::Config1, 0x1FFF0033)?;
        let res = self.read_register(Register::Config2)?;
        assert_eq!(0x0, res);
        self.write_register(Register::Config2, 0x17FF0033)?;
        self.write_register(Register::Argument, 0x0)?;
        self.write_register(Register::CommandAndTransferMode, 0x0)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x1, res);
        self.write_register(Register::InterruptStatus, 0x1)?;
        let res = self.read_register(Register::Command)?;
        assert_eq!(0x47, res);
        self.write_register(Register::Command, 0xE0047)?;

        // Do some sort of memory training?
        let mut current_val = None;
        loop {
            self.write_register(Register::Argument, 0x40000080)?;
            self.write_register(Register::CommandAndTransferMode, 0x1020000)?;
            let res = self.read_register(Register::InterruptStatus)?;
            assert_eq!(0x0, res);
            let res = self.read_register(Register::InterruptStatus)?;
            assert_eq!(0x1, res);
            self.write_register(Register::InterruptStatus, 0x1)?;
            let res = self.read_register(Register::Response0And1)?;

            if current_val.is_none() {
                assert_eq!(0xFF8080, res);
                current_val = Some(res);
                println!("Current val: {res:#08X}");
            }

            if let Some(val) = current_val {
                if val != res {
                    assert_eq!(0xC0FF8080, res);
                    println!("Val changed, prev: {val:#08X}, now: {res:#08X}");
                    break;
                }
            }

            std::thread::sleep(std::time::Duration::from_micros(100));
        }

        self.write_register(Register::Argument, 0x0)?;
        self.write_register(Register::CommandAndTransferMode, 0x2090000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x1, res);
        self.write_register(Register::InterruptStatus, 0x1)?;
        let res = self.read_register(Register::Response0And1)?;
        assert_eq!(0xF4E59BF, res);
        let res = self.read_register(Register::Response2And3)?;
        assert_eq!(0x3932009D, res);
        let res = self.read_register(Register::Response4And5)?;
        assert_eq!(0x30303847, res);
        let res = self.read_register(Register::Response6And7)?;
        assert_eq!(0x110100, res);

        self.write_register(Register::Argument, 0xA0000)?;
        self.write_register(Register::CommandAndTransferMode, 0x31A0000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x1, res);
        self.write_register(Register::InterruptStatus, 0x1)?;

        self.write_register(Register::Argument, 0xA0000)?;
        self.write_register(Register::CommandAndTransferMode, 0x71A0000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x1, res);
        self.write_register(Register::InterruptStatus, 0x1)?;

        self.write_register(Register::Argument, 0x3B70200)?;
        self.write_register(Register::CommandAndTransferMode, 0x61B0000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x3, res);
        self.write_register(Register::InterruptStatus, 0x1)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x2, res);
        self.write_register(Register::InterruptStatus, 0x2)?;
        let res = self.read_register(Register::Reg_0A)?;
        assert_eq!(0x800000, res);
        self.write_register(Register::Reg_0A, 0x800020)?;

        self.write_register(Register::Argument, 0x200)?;
        self.write_register(Register::CommandAndTransferMode, 0x101A0000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x1, res);
        self.write_register(Register::InterruptStatus, 0x1)?;

        self.write_register(Register::Argument, 0x3B90100)?;
        self.write_register(Register::CommandAndTransferMode, 0x61B0000)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x0, res);
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x3, res);
        self.write_register(Register::InterruptStatus, 0x1)?;
        let res = self.read_register(Register::InterruptStatus)?;
        assert_eq!(0x2, res);
        self.write_register(Register::InterruptStatus, 0x2)?;
        let res = self.read_register(Register::Reg_0F)?;
        assert_eq!(0x0, res);

        self.write_register(Register::Reg_0F, 0x80000)?;
        self.write_register(Register::Reg_0A, 0x800024)?;
        self.write_register(Register::XipOutputDelay, 0x70001)?;
        let res = self.read_register(Register::XipOutputDelay)?;
        assert_eq!(0x70001, res);
        let res = self.read_register(Register::Command)?;
        assert_eq!(0xE0047, res);
        self.write_register(Register::Command, 0xE0047)?;
        let res = self.read_register(Register::Command)?;
        assert_eq!(0xE0047, res);
        self.write_register(Register::Command, 0xE0043)?;
        self.write_register(Register::Command, 0xE0203)?;
        self.write_register(Register::Command, 0xE0207)?;
        self.write_register(Register::Reg_01, 0x10200)?;

        Ok(())
    }

    /// Initialize the device
    /// 
    /// This performs:
    /// 1. Hardware initialization (GPIO, SPI, reset)
    /// 2. Sends initialization command
    /// 3. Runs sanity checks
    /// 4. Send init sequence
    pub fn init(&mut self) -> Result<(), Error> {
        if self.initialized {
            return Ok(());
        }
        
        // Step 1: Initialize hardware backend
        self.backend.initialize()?;
        
        // Step 2: Send initialization command
        // Write 0x00000003 to register 0x44
        self.backend.write_register(Register::InitCommand, 0x00000003)?;
        
        // Step 3: Sanity checks
        self.sanity_check()?;

        // Step 4: Init sequence
        self.init_sequence()?;
        
        self.initialized = true;
        Ok(())
    }
    
    /// Run sanity checks to verify communication
    fn sanity_check(&mut self) -> Result<(), Error> {
        const TEST_VAL_1: u32 = 0x12345678;
        const TEST_VAL_2: u32 = 0xEDCBA987;
        for test_value in [TEST_VAL_1, TEST_VAL_2, TEST_VAL_1, TEST_VAL_2] {
            self.backend.write_register(Register::Argument, test_value)?;
            let response1 = self.backend.read_register(Register::Argument)?;
            
            if response1 != test_value {
                return Err(Error::SanityCheckFailed {
                    expected: test_value,
                    actual: response1,
                });
            }
        }

        Ok(())
    }
    
    /// Write a value to a register
    pub fn write_register(&mut self, register: Register, value: u32) -> Result<(), Error> {
        self.backend.write_register(register, value)
    }
    
    /// Read a value from a register
    pub fn read_register(&mut self, register: Register) -> Result<u32, Error> {
        self.backend.read_register(register)
    }
    
    /// Read a 512-byte block
    pub fn read_data(&mut self, register: Register, buffer: &mut [u8]) -> Result<(), Error> {
        self.backend.read_data(register, buffer)
    }
    
    /// Read the present state register
    pub fn read_present_state(&mut self) -> Result<u32, Error> {
        self.read_register(Register::PresentState)
    }
    
    /// Read the page number / interrupt status register (0x0C)
    pub fn read_interrupt_status(&mut self) -> Result<u32, Error> {
        self.read_register(Register::InterruptStatus)
    }
    
    /// Read the command / status config register (0x0B)
    pub fn read_status_config(&mut self) -> Result<u32, Error> {
        self.read_register(Register::Command)
    }
    
    /// Read a response register
    pub fn read_response(&mut self, index: u8) -> Result<u32, Error> {
        let register = match index {
            0 => Register::Response0And1,
            1 => Register::Response2And3,
            2 => Register::Response4And5,
            3 => Register::Response6And7,
            _ => return Err(Error::RegisterAccessFailed),
        };
        
        self.read_register(register)
    }
    
    /// Check if initialization is complete
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    
    pub fn poll_for_value(&mut self, register: Register, value: u32) -> Result<(), Error> {
        const MAX_POLLS: u32 = 10;
        for _ in 0..MAX_POLLS {
            let status_value = self.read_register(register)?;
            if status_value == value {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        return Err(Error::Timeout);
    }

    /// Read a page from the eMMC chip
    /// 
    /// This implements the full page read sequence based on protocol trace analysis:
    /// 1. Clear/reset status
    /// 2. Set page address
    /// 3. Set transfer configuration
    /// 4. Poll for command accepted
    /// 5. Poll for data ready and acknowledge
    /// 6. Read 512 bytes from data FIFO
    /// 7. Acknowledge transfer complete
    ///
    /// # Arguments
    /// * `page_number` - The page number to read
    /// * `buffer` - Buffer to store the 512-byte page
    pub fn read_page(
        &mut self,
        page_number: u32,
        buffer: &mut [u8; 512]
    ) -> Result<(), Error> {
        // Step 1: Clear/reset status
        self.write_register(Register::InterruptStatus, status::STATUS_CLEAR)?;

        // Step 2: Set page address
        self.write_register(Register::Argument, page_number)?;
        
        // Step 3: Set transfer configuration (observed value from protocol trace)
        self.write_register(Register::CommandAndTransferMode, transfer_config::PAGE_READ)?;
        
        // Step 4: Poll for command accepted
        self.poll_for_value(Register::InterruptStatus, status::CMD_ACCEPTED)?;
        
        // Step 5: Poll for data ready and send interrupt acknowledge
        self.poll_for_value(Register::InterruptStatus, status::DATA_READY)?;
        self.write_register(Register::InterruptStatus, status::DATA_READY)?;
        
        // Step 6: Read 512-byte block from data FIFO
        self.read_data(Register::DataFifo, buffer)?;
        
        // Step 7: Read transfer complete status and send interrupt acknowledge
        let _status_value = self.read_register(Register::InterruptStatus)?;
        self.write_register(Register::InterruptStatus, status::TRANSFER_COMPLETE)?;
        
        Ok(())
    }
    
    /// Erase a page from the eMMC chip (STUB)
    /// 
    /// This implements the page erase sequence (to be completed based on protocol analysis):
    /// 1. Clear/reset status
    /// 2. Set page address
    /// 3. Set erase transfer configuration
    /// 4. Poll for command accepted
    /// 5. Poll for erase complete
    /// 6. Acknowledge completion
    ///
    /// # Arguments
    /// * `page_number` - The page number to erase
    /// 
    /// # Note
    /// This is currently a STUB implementation. The actual protocol sequence needs to be
    /// determined through hardware testing and protocol trace analysis.
    pub fn erase_page(
        &mut self,
        page_number: u32
    ) -> Result<(), Error> {
        // TODO: Implement actual erase sequence once protocol is understood
        // The sequence will likely be similar to read_page but with different
        // transfer configuration and status polling
        
        // Step 1: Clear/reset status
        self.write_register(Register::InterruptStatus, status::STATUS_CLEAR)?;

        // Step 2: Set page address to erase
        self.write_register(Register::Argument, page_number)?;
        
        // Step 3: Set erase transfer configuration
        // TODO: Determine the correct transfer configuration value for erase operations
        // This value needs to be captured from actual hardware protocol traces
        const ERASE_TRANSFER_CONFIG: u32 = 0x00000000; // PLACEHOLDER - needs actual value
        self.write_register(Register::CommandAndTransferMode, ERASE_TRANSFER_CONFIG)?;
        
        // Step 4: Poll for command accepted
        self.poll_for_value(Register::InterruptStatus, status::CMD_ACCEPTED)?;
        
        // Step 5: Poll for erase complete
        // TODO: Determine the correct status value for erase completion
        // Erasing typically takes longer than reading
        self.poll_for_value(Register::InterruptStatus, status::TRANSFER_COMPLETE)?;
        self.write_register(Register::InterruptStatus, status::TRANSFER_COMPLETE)?;
        
        println!("WARNING: erase_page is a STUB - protocol sequence not yet validated");
        
        Ok(())
    }
    
    /// Write a page to the eMMC chip (STUB)
    /// 
    /// This implements the page write sequence (to be completed based on protocol analysis):
    /// 1. Clear/reset status
    /// 2. Set page address
    /// 3. Set write transfer configuration
    /// 4. Poll for command accepted
    /// 5. Write 512 bytes to data FIFO
    /// 6. Poll for write complete
    /// 7. Acknowledge completion
    ///
    /// # Arguments
    /// * `page_number` - The page number to write
    /// * `buffer` - Buffer containing the 512-byte page to write
    /// 
    /// # Note
    /// This is currently a STUB implementation. The actual protocol sequence needs to be
    /// determined through hardware testing and protocol trace analysis.
    pub fn write_page(
        &mut self,
        page_number: u32,
        buffer: &[u8; 512]
    ) -> Result<(), Error> {
        // TODO: Implement actual write sequence once protocol is understood
        // The sequence will likely be similar to read_page but with data output
        // instead of data input

        // Step 1: Clear/reset status
        self.write_register(Register::InterruptStatus, status::STATUS_CLEAR)?;

        // Step 2: Set page address to write
        self.write_register(Register::Argument, page_number)?;
        
        // Step 3: Set write transfer configuration
        // TODO: Determine the correct transfer configuration value for write operations
        // This value needs to be captured from actual hardware protocol traces
        const WRITE_TRANSFER_CONFIG: u32 = 0x00000000; // PLACEHOLDER - needs actual value
        self.write_register(Register::CommandAndTransferMode, WRITE_TRANSFER_CONFIG)?;
        
        // Step 4: Poll for command accepted
        self.poll_for_value(Register::InterruptStatus, status::CMD_ACCEPTED)?;
        
        // Step 5: Write 512-byte block to data FIFO
        // TODO: Implement write_data method in backend trait
        // For now, this is a placeholder that would trigger a compile error
        // if uncommented without implementing the backend method
        // self.backend.write_data(Register::DataFifo, buffer)?;
        
        // Step 6: Poll for write complete
        // TODO: Determine if there's a specific status for write ready/complete
        self.poll_for_value(Register::InterruptStatus, status::TRANSFER_COMPLETE)?;
        self.write_register(Register::InterruptStatus, status::TRANSFER_COMPLETE)?;
        
        println!("WARNING: write_page is a STUB - protocol sequence not yet validated");
        
        // Prevent unused variable warning
        let _ = buffer;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock backend for testing
    struct MockBackend {
        registers: std::collections::HashMap<u8, u32>,
        initialized: bool,
    }
    
    impl MockBackend {
        fn new() -> Self {
            Self {
                registers: std::collections::HashMap::new(),
                initialized: false,
            }
        }
    }
    
    impl SpiBackend for MockBackend {
        fn write_register(&mut self, register: Register, data: u32) -> Result<(), Error> {
            self.registers.insert(register.address(), data);
            Ok(())
        }
        
        fn read_register(&mut self, register: Register) -> Result<u32, Error> {
            Ok(*self.registers.get(&register.address()).unwrap_or(&0))
        }
        
        fn read_data(&mut self, _register: Register, buffer: &mut [u8]) -> Result<(), Error> {
            buffer.fill(0);
            Ok(())
        }
        
        fn reset(&mut self) -> Result<(), Error> {
            Ok(())
        }
        
        fn initialize(&mut self) -> Result<(), Error> {
            self.initialized = true;
            Ok(())
        }
    }
    
    #[test]
    fn test_read_write() {
        let backend = MockBackend::new();
        let mut reader = EmmcReader::new(backend);
        // reader.init().unwrap();
        
        // Write and read back
        reader.write_register(Register::Argument, 0xDEADBEEF).unwrap();
        let value = reader.read_register(Register::Argument).unwrap();
        assert_eq!(value, 0xDEADBEEF);
    }
}
