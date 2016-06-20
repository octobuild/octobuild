use rustc_serialize::{Decoder, Encodable, Encoder};
use uuid::Uuid;

pub const RPC_BUILDER_UPDATE: &'static str = "/rpc/v1/builder/update";
pub const RPC_BUILDER_LIST: &'static str = "/rpc/v1/builder/list";

pub const RPC_BUILDER_TASK: &'static str = "/rpc/v1/builder/task";
pub const RPC_BUILDER_UPLOAD: &'static str = "/rpc/v1/builder/upload";

#[derive(RustcEncodable, RustcDecodable)]
pub struct BuilderInfo {
    // Agent name
    pub name: String,
    // Agent endpoint
    pub endpoint: String,
    // Agent version
    pub version: String,
    // Agent toolchain list
    pub toolchains: Vec<String>,
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct BuilderInfoUpdate {
    // Hidden unique Id for builder update information
    pub guid: String,
    // Builder information
    pub info: BuilderInfo,
}

impl BuilderInfoUpdate {
    pub fn new(info: BuilderInfo) -> Self {
        BuilderInfoUpdate {
            guid: Uuid::new_v4().to_string(),
            info: info,
        }
    }
}
