use std::env::consts::{ARCH, OS};

include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

pub fn full_version() -> String {
    format!(
        "{}-{}-{}, rev: {}, rustc: {}",
        VERSION,
        ARCH,
        OS,
        &REVISION[0..9],
        RUSTC
    )
}

pub fn short_version() -> String {
    format!("{}/{}", VERSION, &REVISION[0..9])
}
