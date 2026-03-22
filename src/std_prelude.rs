pub mod prelude {
    pub use std::prelude::*;
    pub use std::prelude::rust_2024::derive;
    pub use std::{todo, writeln, write, assert_eq};
    pub use std::marker::{Copy, Send};
    pub use std::ops::{Fn, FnMut};
    pub use std::mem::{drop, size_of};
    pub use std::convert::{From, Into};
    pub use std::result::Result::{self, Err, Ok};
    pub use std::option::Option::{self, Some, None};
    pub use std::time::Duration;
}