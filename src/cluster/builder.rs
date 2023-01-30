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
