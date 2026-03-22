#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_imports)]
#![allow(unused_variables)]

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
pub mod no_std_prelude;
#[cfg(feature = "std")]
pub mod std_prelude;

#[cfg(not(feature = "std"))]
pub use no_std_prelude::no_std_prelude as prelude;
#[cfg(feature = "std")]
pub use std_prelude::prelude;

pub mod error;
pub mod i2c;
pub mod spi;

pub use embedded_hal;
pub use embedded_hal::delay::DelayNs as DelayTrait;

pub use i2c::isd9160::{Isd9160, Isd9160Sounds};
#[cfg(feature = "ftdi")]
pub use i2c::i2c_bitbang::I2cFtBitbang;
#[cfg(feature = "ftdi")]
pub use libftd2xx::{BitMode, Ft4232h, FtdiCommon};

