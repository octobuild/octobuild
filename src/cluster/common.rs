use rustc_serialize::{Decoder, Encodable, Encoder};
use uuid::Uuid;

#[derive(RustcEncodable, RustcDecodable)]
pub struct BuilderInfo {
    // Agent name
    pub name: String,
    // Agent endpoints
    pub endpoints: Vec<String>,
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct BuilderInfoUpdate {
    // Hidden unique Id for builder update information
    pub guid: String,
    // Builder information
    pub info: BuilderInfo,
}

impl BuilderInfoUpdate {
    pub fn new(info: BuilderInfo) -> BuilderInfoUpdate {
        BuilderInfoUpdate {
            guid: Uuid::new_v4().to_string(),
            info: info,
        }
    }
}