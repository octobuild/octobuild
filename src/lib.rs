include!(concat!(env!("OUT_DIR"), "/version.rs"));

#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

extern crate fern;
extern crate rustc_serialize;
extern crate time;
extern crate uuid;
extern crate regex;
#[cfg(windows)]
extern crate winapi;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub mod cache;
pub mod cluster {
	pub mod common;
}
pub mod common;
pub mod compiler;
pub mod config;
pub mod hostname;
pub mod lazy;
pub mod utils;
pub mod version;
pub mod io {
	pub mod tempfile;
	pub mod binary;
	pub mod counter;
	pub mod filecache;
	pub mod hashwriter;
	pub mod memcache;
	pub mod memstream;
	pub mod statistic;
}
pub mod xg {
	pub mod parser;
}
pub mod vs {
	pub mod compiler;
	pub mod prepare;
	pub mod postprocess;
}
pub mod clang {
	pub mod compiler;
	pub mod prepare;
}
pub mod cmd {
	pub mod windows;
	pub mod unix;
	pub mod native;
}
pub mod filter {
	pub mod comments;
}
