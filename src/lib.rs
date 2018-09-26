include!(concat!(env!("OUT_DIR"), "/version.rs"));
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

extern crate byteorder;
extern crate capnp;
extern crate crossbeam;
extern crate crypto;
extern crate fern;
extern crate hyper;
extern crate ipc;
extern crate libc;
extern crate local_encoding;
extern crate petgraph;
extern crate rand;
extern crate regex;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate tempdir;
extern crate time;
extern crate uuid;
#[cfg(windows)]
extern crate winapi;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

#[allow(dead_code)]
pub mod builder_capnp {
    include!(concat!(env!("OUT_DIR"), "/builder_capnp.rs"));
}

pub mod cache;
pub mod cluster {
    pub mod builder;
    pub mod client;
    pub mod common;
}
pub mod compiler;
pub mod config;
pub mod lazy;
pub mod utils;
pub mod version;
pub mod io {
    pub mod binary;
    pub mod counter;
    pub mod filecache;
    pub mod memcache;
    pub mod memstream;
    pub mod statistic;
    pub mod tempfile;
}
pub mod xg {
    pub mod parser;
}
pub mod vs {
    pub mod compiler;
    pub mod postprocess;
    pub mod prepare;
}
pub mod clang {
    pub mod compiler;
    pub mod prepare;
}
pub mod cmd {
    pub mod native;
    pub mod unix;
    pub mod windows;
}
pub mod simple;
pub mod worker;
