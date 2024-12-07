use thiserror::Error as DeriveError;
use libftd2xx::TimeoutError as FtdiTimeout;
use libftd2xx::FtStatus;

#[derive(DeriveError, Debug)]
pub enum Error {
    #[error("Not implemented Error")]
    Todo,
    #[error("FTDI Timeout")]
    DeviceTimeout(#[from] FtdiTimeout),
    #[error("FT Status")]
    FtStatus(#[from] FtStatus),
}