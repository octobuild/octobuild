use std::env;
use std::fs::File;
use std::io::Error;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use rustc_version::version;

fn load_revision() -> Result<String, Error> {
    let output = Command::new("git")
        .arg("log")
        .arg("-n1")
        .arg("--format=%H")
        .output()?;
    Ok(String::from_utf8(output.stdout).unwrap().trim().to_string())
}

fn save_version() -> Result<(), Error> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.rs");
    let mut f = File::create(&dest_path).unwrap();
    f.write_all(
        &format!(
            r#"
pub const REVISION: &str = "{revision}";
pub const RUSTC: &str = "{rustc}";
"#,
            revision = load_revision()?,
            rustc = version().unwrap(),
        )
        .into_bytes(),
    )
}

fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("src/schema")
        .file("src/schema/builder.capnp")
        .run()
        .unwrap();
    save_version().unwrap();
}
