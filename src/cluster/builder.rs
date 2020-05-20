use std::io;

use serde::{Deserialize, Serialize};

use crate::compiler::OutputInfo;

#[derive(Serialize, Deserialize, Debug)]
pub struct CompileRequest {
    pub toolchain: String,
    pub args: Vec<String>,
    pub preprocessed_data: Vec<u8>,
    pub precompiled_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CompileResponse {
    Success(OutputInfo, Vec<u8>),
    Err(String),
}

impl From<Result<(OutputInfo, Vec<u8>), io::Error>> for CompileResponse {
    fn from(result: Result<(OutputInfo, Vec<u8>), io::Error>) -> Self {
        match result {
            Ok((output, content)) => CompileResponse::Success(output, content),
            Err(v) => CompileResponse::Err(v.to_string()),
        }
    }
}
