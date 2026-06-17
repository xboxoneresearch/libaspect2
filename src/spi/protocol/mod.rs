//! Protocol module - hardware-independent SPI protocol implementations
//!
//! This module defines the protocol structures and operations without
//! depending on any specific hardware backend (FTDI, embedded-hal, etc.)
pub mod constants;
pub mod nor;
pub mod transaction;
