#![feature(test)]

include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub mod cache;
pub mod common;
pub mod compiler;
pub mod utils;
pub mod wincmd;
pub mod version;
pub mod io {
	pub mod tempfile;
	pub mod binary;
}
pub mod xg {
	pub mod parser;
}
pub mod vs {
	pub mod compiler;
	mod prepare;
	mod postprocess;
}
