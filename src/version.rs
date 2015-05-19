use std::env::consts::{ARCH, OS};

include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub fn full_version() -> String {
    format!("{}-{}-{} {}", VERSION, ARCH, OS, &REVISION[0..9])
}