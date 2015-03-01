#![feature(core)]
#![feature(collections)]
#![feature(hash)]
#![feature(old_io)]
#![feature(old_path)]
#![feature(os)]
#![feature(std_misc)]
#![feature(int_uint)]
pub mod cache;
pub mod common;
pub mod compiler;
pub mod graph;
pub mod utils;
pub mod wincmd;
pub mod io {
	pub mod tempfile;
}
pub mod xg {
	pub mod parser;
}
pub mod vs {
	pub mod compiler;
	mod prepare;
	mod postprocess;
}