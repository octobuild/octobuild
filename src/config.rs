use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::OnceLock;

use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;

#[derive(Copy, Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CacheMode {
    None,
    ReadOnly,
    ReadWrite,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub cache: PathBuf,
    pub cache_mode: CacheMode,
    pub cache_limit_mb: u64,
    pub cache_compression_level: u32,
    pub coordinator: Option<url::Url>,
    pub coordinator_bind: SocketAddr,
    pub helper_bind: SocketAddr,
    pub process_limit: usize,
    pub run_second_cpp: bool,
    pub use_response_files: bool,
}

#[must_use]
fn project_dirs() -> &'static directories::ProjectDirs {
    static DIRS: OnceLock<directories::ProjectDirs> = OnceLock::new();
    return DIRS.get_or_init(|| directories::ProjectDirs::from("", "", "octobuild").unwrap());
}

// Windows has 32KB commandline length limit, so we have to use response files to circumvent that.
#[cfg(windows)]
const DEFAULT_USE_RESPONSE_FILES: bool = true;
#[cfg(not(windows))]
const DEFAULT_USE_RESPONSE_FILES: bool = false;

impl Default for Config {
    fn default() -> Self {
        Self {
            cache: project_dirs().cache_dir().into(),
            cache_mode: CacheMode::ReadWrite,
            cache_limit_mb: 64 * 1024,
            cache_compression_level: 1,
            coordinator: None,
            coordinator_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3000)),
            helper_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
            process_limit: num_cpus::get(),
            run_second_cpp: true,
            use_response_files: DEFAULT_USE_RESPONSE_FILES,
        }
    }
}

impl Config {
    pub fn load() -> crate::Result<Config> {
        let mut figment = Figment::from(Serialized::defaults(Config::default()));

        for path in vec![global_config_path(), local_config_path()]
            .into_iter()
            .flatten()
        {
            figment = figment.merge(Yaml::file(path));
        }

        Ok(figment.merge(Env::prefixed("OCTOBUILD_")).extract()?)
    }

    pub fn print_help(&self, executable: &str, out: &mut impl Write) -> crate::Result<()> {
        writeln!(out)?;
        writeln!(out, "Usage:")?;
        writeln!(out, "  {} <file>", executable)?;
        writeln!(out, "  {} /reset", executable)?;
        writeln!(out,)?;
        writeln!(out, "Octobuild configuration:")?;
        writeln!(
            out,
            "  system config path: {}",
            global_config_path()
                .and_then(|v| Some(v.to_str()?.to_string()))
                .unwrap_or_else(|| "none".to_string())
        )?;
        writeln!(
            out,
            "  user config path:   {}",
            local_config_path()
                .and_then(|v| Some(v.to_str()?.to_string()))
                .unwrap_or_else(|| "none".to_string())
        )?;
        writeln!(out)?;
        writeln!(out, "Current configuration:")?;
        self.show(out)?;
        writeln!(out)?;

        Ok(())
    }

    fn show(&self, out: &mut impl Write) -> crate::Result<()> {
        Ok(writeln!(out, "{}", serde_yaml::to_string(self).unwrap())?)
    }
}

fn local_config_path() -> Option<PathBuf> {
    Some(project_dirs().config_dir().join("octobuild.conf"))
}

#[cfg(windows)]
fn global_config_path() -> Option<PathBuf> {
    use std::env;

    Some(
        PathBuf::from(env::var("ProgramData").ok()?)
            .join("octobuild")
            .join("octobuild.conf"),
    )
}

#[cfg(unix)]
fn global_config_path() -> Option<PathBuf> {
    Some(
        PathBuf::from("/etc")
            .join("octobuild")
            .join("octobuild.conf"),
    )
}
