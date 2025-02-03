use std::ffi::OsString;

#[cfg(not(windows))]
pub mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;

pub fn join<'a, I: IntoIterator<Item = &'a OsString>>(words: I) -> crate::Result<OsString> {
    let result = shlex::try_join(words.into_iter().map(|x| x.to_str().unwrap()))?;
    Ok(OsString::from(result))
}
