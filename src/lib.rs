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
	pub mod filecache;
	pub mod hashwriter;
	pub mod memcache;
}
pub mod xg {
	pub mod parser;
}
pub mod vs {
	pub mod compiler;
	pub mod prepare;
	pub mod postprocess;
}
