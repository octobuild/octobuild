use std::env;
use std::io::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn save_platform() -> Result<(), Error> {
    let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = env::var("PROFILE").unwrap();
    let dest_path = Path::new(&root_dir).join("target").join(&profile).join("target.txt");
    let mut f = try!(File::create(&dest_path));
    f.write_all(env::var("TARGET").unwrap().as_bytes())
}

fn load_revision() -> Result<String, Error> {
    let output = try! (Command::new("git")
                     .arg("log")
                     .arg("-n1")
                     .arg("--format=%H")
                     .output());
    Ok(String::from_utf8(output.stdout).unwrap())
}

fn save_version() -> Result<(), Error> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version.rs");
    let mut f = File::create(&dest_path).unwrap();
    f.write_all(&format!(r#"
pub const REVISION: &'static str = "{revision}";
    "#, revision = try!(load_revision())).into_bytes())
}

fn main() {
    save_platform().unwrap();
    save_version().unwrap();
}
