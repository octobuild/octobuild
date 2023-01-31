use std::ffi::{OsStr, OsString};

#[cfg(not(windows))]
pub mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

#[must_use]
pub fn join<'a, I: IntoIterator<Item = &'a OsString>>(words: I) -> OsString {
    words
        .into_iter()
        .map(quote)
        .collect::<Vec<OsString>>()
        .join(OsStr::new(" "))
}
