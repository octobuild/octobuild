#[cfg(windows)]
use std::env;
#[cfg(windows)]
use std::fs::File;
#[cfg(windows)]
use std::io::Read;
#[cfg(windows)]
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn get_dest_dir() -> PathBuf {
    //<root or manifest path>/target/<profile>/
    let manifest_dir_string = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_type = env::var("PROFILE").unwrap();
    PathBuf::from(manifest_dir_string)
        .join("target")
        .join(build_type)
}

#[cfg(windows)]
fn copy_redist_msm(dest_dir: &Path) {
    let tool = cc::windows_registry::find_tool("x86_64-msvc", "cl.exe").unwrap();

    let vc_dir = tool
        .path()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();

    let version_path = vc_dir
        .join("Auxiliary")
        .join("Build")
        .join("Microsoft.VCRedistVersion.default.txt");

    let mut version_file = File::open(version_path).unwrap();
    let mut version = String::new();
    version_file.read_to_string(&mut version).unwrap();
    let version = version.trim();

    let msm_dir = vc_dir
        .join("Redist")
        .join("MSVC")
        .join(version)
        .join("MergeModules");

    let msm_suffix = "CRT_x64.msm";

    for f in msm_dir.read_dir().unwrap() {
        let f = f.unwrap();
        if f.file_name().to_string_lossy().ends_with(msm_suffix) {
            std::fs::copy(f.path(), dest_dir.join("vcredist.msm")).unwrap();
            return;
        }
    }

    panic!("Failed to find '*{msm_suffix}' {msm_dir:?}");
}

fn main() {
    #[cfg(windows)]
    {
        let dest_dir = get_dest_dir();
        copy_redist_msm(&dest_dir);
    }
}
