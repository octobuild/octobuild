use std::env::consts::{ARCH, OS};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[must_use]
pub fn full() -> String {
    format!("{VERSION}-{ARCH}-{OS}")
}
