pub mod prelude {
    pub use crate::clock::{ClockTrait, StdClock, Timer};
    pub use embedded_hal::delay::DelayNs;
    pub use std::convert::{From, Into};
    pub use std::marker::{Copy, Send};
    pub use std::mem::{drop, size_of};
    pub use std::ops::{Fn, FnMut};
    pub use std::option::Option::{self, None, Some};
    pub use std::prelude::rust_2024::derive;
    pub use std::prelude::*;
    pub use std::result::Result::{self, Err, Ok};
    pub use std::time::{Duration, Instant};
    pub use std::{assert_eq, todo, write, writeln};
}
