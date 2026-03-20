#![allow(dead_code)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(unused_imports)]
#![allow(unused_variables)]

pub mod error;
pub mod i2c;
pub mod spi;

pub use embedded_hal::i2c as eh_i2c;
pub use embedded_hal::spi as eh_spi;
pub use i2c::i2c_bitbang::I2cFtBitbang;
pub use i2c::isd9160::{Isd9160, Isd9160Sounds};
pub use libftd2xx::{BitMode, Ft4232h, FtdiCommon};
