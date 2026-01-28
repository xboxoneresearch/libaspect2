pub mod i2c;
pub mod spi;
pub mod error;

pub use embedded_hal::i2c as eh_i2c;
pub use embedded_hal::spi as eh_spi;
pub use i2c::i2c_bitbang::I2cFtBitbang;
pub use i2c::isd9160::{Isd9160, Isd9160Sounds};
pub use libftd2xx::{Ft4232h, FtdiCommon, BitMode};
