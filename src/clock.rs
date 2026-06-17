use embedded_hal::delay::DelayNs;
pub use embedded_timers::clock::Clock as ClockTrait;
pub use embedded_timers::delay::Delay;
pub use embedded_timers::instant::Instant;
pub use embedded_timers::timer::Timer;

#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct StdClock;

#[cfg(feature = "std")]
impl ClockTrait for StdClock {
    type Instant = std::time::Instant;

    fn now(&self) -> Self::Instant {
        std::time::Instant::now()
    }
}

#[cfg(feature = "std")]
impl DelayNs for StdClock {
    fn delay_ns(&mut self, ns: u32) {
        Delay::new(&StdClock).delay_ns(ns);
    }
}
