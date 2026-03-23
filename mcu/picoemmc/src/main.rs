#![no_std]
#![no_main]

use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_timers::instant::Instant64;
use hal::timer::monotonic::Monotonic;
use embedded_timers::clock::Clock as ETClock;
use libaspect2::clock::DelayNs;
// Ensure we halt the program on panic (if we don't mention this crate it won't
// be linked)
use panic_halt as _;

// Alias for our HAL crate
use rp2040_hal::{self as hal, Timer};

// A shorter alias for the Peripheral Access Crate, which provides low-level
// register access
use hal::pac;

// Some traits we need
use hal::fugit::RateExtU32;
use rp2040_hal::clocks::Clock;
use usb_device::class_prelude::*;
use usb_device::prelude::*;

use libaspect2::spi::emmc_reader::{EmmcReader, EraseType};
use libaspect2::spi::backend::eh::Eh1SpiBackend;

const SPI_BITLEN: u8 = 8;

#[derive(Clone, Copy)]
pub struct PicoClockDelay
{
    _timer: u8,
}

impl PicoClockDelay
{
    pub fn new() -> Self {
        Self { _timer: 0 }
    }
}

impl ETClock for PicoClockDelay
{
    type Instant = Instant64<1000>;
    fn now(&self) -> Self::Instant {
        Instant64::<1000>::new(0)
    }
}

impl DelayNs for PicoClockDelay
{
    fn delay_ns(&mut self, _ns: u32) {
        
    }
}

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

/// External high-speed crystal on the Raspberry Pi Pico board is 12 MHz. Adjust
/// if your board has a different frequency
const XTAL_FREQ_HZ: u32 = 12_000_000u32;

/// Entry point to our bare-metal application.
///
/// The `#[rp2040_hal::entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables and the spinlock are initialised.
///
/// The function configures the RP2040 peripherals, then writes to the UART in
/// an infinite loop.
#[rp2040_hal::entry]
fn main() -> ! {
    // Grab our singleton objects
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();

    // Set up the watchdog driver - needed by the clock setup code
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    // Configure the clocks
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .unwrap();

    // The single-cycle I/O block controls our GPIO pins
    let sio = hal::Sio::new(pac.SIO);

    // Set the pins to their default state
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // --- Timers ---

    let mut timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

    let mut _delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());
    let mut _monotonic = Monotonic::new(timer, timer.alarm_0().unwrap());
    let clockdelay = PicoClockDelay::new();

    // --- SPI bus setup (use GPIO2-5 for SPI0) ---
    let spi_miso = pins.gpio2.into_function::<hal::gpio::FunctionSpi>();
    let spi_sck = pins.gpio3.into_function::<hal::gpio::FunctionSpi>();
    let spi_mosi = pins.gpio4.into_function::<hal::gpio::FunctionSpi>();
    let spi_cs = pins.gpio5.into_push_pull_output();
    
    let spi_bus = hal::spi::Spi::<_, _, _, SPI_BITLEN>::new(pac.SPI0, (spi_sck, spi_mosi, spi_miso));
    let spi_bus = spi_bus.init(
        &mut pac.RESETS,
        clocks.peripheral_clock.freq(),
        1_000_000u32.Hz(),
        &embedded_hal::spi::MODE_0,
    );
    let spi = ExclusiveDevice::new(spi_bus, spi_cs, &mut timer).unwrap();

    // --- SMC reset ---
    let smc_rst = pins.gpio0.into_push_pull_output_in_state(rp2040_hal::gpio::PinState::High);

    // --- USB-CDC setup ---
    let usb_bus =
        UsbBusAllocator::new(hal::usb::UsbBus::new(
            pac.USBCTRL_REGS,
            pac.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut pac.RESETS,
        ));
    let mut serial = usbd_serial::SerialPort::new(&usb_bus);
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
//      .manufacturer("libaspect2")
//      .product("picoemmc")
//      .serial_number("0001")
        .device_class(usbd_serial::USB_CLASS_CDC)
        .build();

    // --- eMMC Reader setup ---
    let dummy_pin = pins.gpio17.into_push_pull_output();

    let mut emmc = EmmcReader::new(
        Eh1SpiBackend::new(spi, Some(smc_rst), Some(dummy_pin), clockdelay),
        clockdelay,
    );
    let _ = emmc.init(); // Try to init card (ignore error for now)

    // --- Main USB serial RPC loop ---
    let mut buf = [0u8; 576];
    loop {
        if !usb_dev.poll(&mut [&mut serial]) {
            continue;
        }
        match serial.read(&mut buf) {
            Ok(count) if count > 0 => {
                // Very basic protocol: [cmd, ...payload]
                match buf[0] {
                    0x01 => { // Dump fuses
                        // TODO: implement fuse dump
                        let _ = serial.write(b"fuse:stub\n");
                    }
                    0x02 => { // Read page: [0x02, lba:4]
                        if count >= 5 {
                            let lba = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                            let mut page = [0u8; 512];
                            if emmc.read_page(lba, &mut page).is_ok() {
                                let _ = serial.write(&page);
                            } else {
                                let _ = serial.write(b"ERR\n");
                            }
                        }
                    }
                    0x03 => { // Write page: [0x03, lba:4, data:512]
                        if count >= 5+512 {
                            let lba = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                            let mut page = [0u8; 512];
                            page.copy_from_slice(&buf[5..5+512]);
                            if emmc.write_page(lba, &page).is_ok() {
                                let _ = serial.write(b"OK\n");
                            } else {
                                let _ = serial.write(b"ERR\n");
                            }
                        }
                    }
                    0x04 => { // Erase: [0x04, start:4, len:4]
                        if count >= 9 {
                            let start = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]) as u64;
                            let len = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as u64;
                            if emmc.erase(EraseType::Erase, start, len).is_ok() {
                                let _ = serial.write(b"OK\n");
                            } else {
                                let _ = serial.write(b"ERR\n");
                            }
                        }
                    }
                    _ => {
                        let _ = serial.write(b"?\n");
                    }
                }
            }
            _ => {}
        }
    }
}