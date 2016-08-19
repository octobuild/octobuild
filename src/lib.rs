include!(concat!(env!("OUT_DIR"), "/version.rs"));
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

extern crate local_encoding;
extern crate byteorder;
extern crate capnp;
extern crate fern;
extern crate hyper;
extern crate crypto;
extern crate rustc_serialize;
extern crate tempdir;
extern crate time;
extern crate uuid;
extern crate rand;
extern crate petgraph;
extern crate regex;
#[cfg(windows)]
extern crate winapi;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[allow(dead_code)]
pub mod builder_capnp {
    include!(concat!(env!("OUT_DIR"), "/builder_capnp.rs"));
}

pub mod cache;
pub mod cluster {
    pub mod common;
    pub mod client;
    pub mod builder;
}
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
pub mod worker;
