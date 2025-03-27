use bincode::{Decode, Encode};

use crate::compiler::OutputInfo;

#[derive(Decode, Encode, Debug)]
pub struct CompileRequest {
    pub toolchain: String,
    pub args: Vec<String>,
    pub preprocessed_data: Vec<u8>,
    pub precompiled_hash: Option<String>,
}

#[derive(Decode, Encode, Debug)]
pub enum CompileResponse {
    Success(OutputInfo),
    Err(String),
}

impl From<crate::Result<OutputInfo>> for CompileResponse {
    fn from(result: crate::Result<OutputInfo>) -> Self {
        match result {
            Ok(output) => CompileResponse::Success(output),
            Err(v) => CompileResponse::Err(v.to_string()),
        }
    }
}
