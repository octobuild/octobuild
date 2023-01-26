use figment::providers::{Env, Format, Serialized, Yaml};
use figment::Figment;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub cache: PathBuf,
    pub cache_limit_mb: u64,
    pub coordinator: Option<url::Url>,
    pub coordinator_bind: SocketAddr,
    pub helper_bind: SocketAddr,
    pub process_limit: usize,
    pub run_second_cpp: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache: dirs::cache_dir().unwrap().join("octobuild"),
            cache_limit_mb: 64 * 1024,
            coordinator: None,
            coordinator_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3000)),
            helper_bind: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)),
            process_limit: num_cpus::get(),
            run_second_cpp: true,
        }
    }
}

impl Config {
    pub fn load() -> figment::error::Result<Config> {
        let mut figment = Figment::from(Serialized::defaults(Config::default()));

        for path in vec![global_config_path(), local_config_path()]
            .into_iter()
            .flatten()
        {
            figment = figment.merge(Yaml::file(path));
        }

        figment.merge(Env::prefixed("OCTOBUILD_")).extract()
    }

    pub fn help() {
        println!("Octobuild configuration:");
        println!(
            "  system config path: {}",
            global_config_path()
                .and_then(|v| Some(v.to_str()?.to_string()))
                .unwrap_or_else(|| "none".to_string())
        );
        println!(
            "  user config path:   {}",
            local_config_path()
                .and_then(|v| Some(v.to_str()?.to_string()))
                .unwrap_or_else(|| "none".to_string())
        );
        println!();
        println!("Current configuration:");
        match Config::load() {
            Ok(c) => {
                c.show();
            }
            Err(e) => {
                println!("  ERROR: {e}");
            }
        }
        println!();
    }

    fn show(&self) {
        println!("{}", serde_yaml::to_string(self).unwrap());
    }
}

fn local_config_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("octobuild").join("octobuild.conf"))
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
