#[cfg(not(windows))]
pub mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;
