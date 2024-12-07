pub enum MmcSpiCommand {
    Argument = 0x2,

    CommandAndTransferMode = 0x3,

    Response0and1 = 0x4,
    Response2and3 = 0x5,
    Response4and5 = 0x6,
    Response6and7 = 0x7,

    PresentState = 0x9,
    InterruptStatus = 0xC
}

pub enum State {
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

pub enum SpiErrors {
    EraseReset = 0xd,
    Error = 0x13,
    CCError = 0x14,
    DeviceEccFailed = 0x15,
    IllegalCommand = 0x16,
    CrcError = 0x17,
    DeviceIsLocked = 0x19,
    BlockLengthError = 0x1d,
    AddressMisalign = 0x1e,
    // AddressOutOfRange = val > 7FFFFFFF
}
