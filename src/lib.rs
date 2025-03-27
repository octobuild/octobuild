use crossbeam_channel::SendError;
use shlex::QuoteError;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::io::filecache::CacheError;
use crate::vs::postprocess::PostprocessError;

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
}

pub mod simple;
pub mod worker;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    BincodeEncode(#[from] bincode::error::EncodeError),
    #[error(transparent)]
    BincodeDecode(#[from] bincode::error::DecodeError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("Found cycles in build graph")]
    CyclesInBuildGraph,
    #[error("Failed to open file {path}: {error}")]
    FileOpen {
        path: PathBuf,
        error: Box<crate::Error>,
    },
    #[error(transparent)]
    Figment(#[from] figment::Error),
    #[error(transparent)]
    FromUtf8(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    FromUtf16(#[from] std::string::FromUtf16Error),
    #[error("Invalid UTF-16 line: odd bytes length")]
    FromUtf16OddLength,
    #[error("Error: {0}")]
    Generic(String),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Build task files not found")]
    NoTaskFiles,
    #[error("Failed to compile {path}: {error}")]
    Compilation {
        path: PathBuf,
        error: Box<crate::Error>,
    },
    #[error("Failed to postprocess {path}: {error}")]
    Postprocess {
        path: PathBuf,
        error: Box<crate::Error>,
    },
    #[error(transparent)]
    PostprocessError(#[from] PostprocessError),
    #[error(transparent)]
    QuoteError(#[from] QuoteError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Toolchain not found: {0}")]
    ToolchainNotFound(PathBuf),
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::Generic(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error::Generic(value.to_string())
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(value: SendError<T>) -> Self {
        Error::Generic(value.to_string())
    }
}

impl Error {
    fn postprocess(path: &Path, error: crate::Error) -> Self {
        Self::Postprocess {
            path: path.into(),
            error: error.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
