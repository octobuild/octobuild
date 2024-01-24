use std::ffi::{OsStr, OsString};

#[cfg(not(windows))]
pub mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

pub fn join<'a, I: IntoIterator<Item = &'a OsString>>(words: I) -> crate::Result<OsString> {
    Ok(words
        .into_iter()
        .map(quote)
        .collect::<crate::Result<Vec<OsString>>>()?
        .join(OsStr::new(" ")))
}
