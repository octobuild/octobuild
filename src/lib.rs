include!(concat!(env!("OUT_DIR"), "/version.rs"));

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub mod cache;
pub mod common;
pub mod compiler;
pub mod config;
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
