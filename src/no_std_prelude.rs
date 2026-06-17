pub mod no_std_prelude {
    pub use crate::clock::{ClockTrait, Timer};
    pub use core::convert::{From, Into};
    pub use core::marker::{Copy, Send};
    pub use core::mem::{drop, size_of};
    pub use core::ops::{Fn, FnMut};
    pub use core::option::Option::{self, None, Some};
    pub use core::prelude::rust_2024::derive;
    pub use core::prelude::*;
    pub use core::result::Result::{self, Err, Ok};
    pub use core::time::Duration;
    pub use core::{assert_eq, todo, write, writeln};
    pub use embedded_hal::delay::DelayNs;
}
