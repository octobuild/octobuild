use bincode::{Decode, Encode};
use uuid::Uuid;

pub const RPC_BUILDER_UPDATE: &str = "/rpc/v1/builder/update";
pub const RPC_BUILDER_LIST: &str = "/rpc/v1/builder/list";

pub const RPC_BUILDER_TASK: &str = "/rpc/v1/builder/task";
pub const RPC_BUILDER_UPLOAD: &str = "/rpc/v1/builder/upload";

#[derive(Decode, Encode)]
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

#[derive(Decode, Encode)]
pub struct BuilderInfoUpdate {
    // Hidden unique Id for builder update information
    pub guid: String,
    // Builder information
    pub info: BuilderInfo,
}

impl BuilderInfoUpdate {
    #[must_use]
    pub fn new(info: BuilderInfo) -> Self {
        BuilderInfoUpdate {
            guid: Uuid::new_v4().to_string(),
            info,
        }
    }
}
