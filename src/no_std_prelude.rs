#![no_std]

pub mod no_std_prelude {
    pub use core::prelude::*;
    pub use core::prelude::rust_2024::derive;
    pub use core::{todo, writeln, write, assert_eq};
    pub use core::marker::{Copy, Send};
    pub use core::ops::{Fn, FnMut};
    pub use core::mem::{drop, size_of};
    pub use core::convert::{From, Into};
    pub use core::result::Result::{self, Err, Ok};
    pub use core::option::Option::{self, Some, None};
    pub use core::time::Duration;
}