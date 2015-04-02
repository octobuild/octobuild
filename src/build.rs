use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = env::var("PROFILE").unwrap();
    let dest_path = Path::new(&root_dir).join("target").join(&profile).join("target.txt");
    let mut f = File::create(&dest_path).unwrap();
    f.write_all(env::var("TARGET").unwrap().as_bytes()).unwrap();
}
