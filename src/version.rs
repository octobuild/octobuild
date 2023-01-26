use std::env::consts::{ARCH, OS};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn full_version() -> String {
    format!("{VERSION}-{ARCH}-{OS}")
}
