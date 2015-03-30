#![feature(core)]
#![feature(collections)]
#![feature(hash)]
#![feature(io)]
#![feature(fs_time)]
#![feature(fs_walk)]
#![feature(old_io)]
#![feature(slice_patterns)]
pub mod cache;
pub mod common;
pub mod compiler;
pub mod graph;
pub mod utils;
pub mod wincmd;
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