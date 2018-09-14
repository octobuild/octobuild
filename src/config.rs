extern crate dirs;
extern crate num_cpus;
extern crate yaml_rust;

use hyper::Url;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::{ErrorKind, Read, Result};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use self::yaml_rust::yaml::Hash;
use self::yaml_rust::{Yaml, YamlEmitter, YamlLoader};

pub struct Config {
    pub coordinator: Option<Url>,
    pub helper_bind: SocketAddr,
    pub coordinator_bind: SocketAddr,

    pub process_limit: usize,
    pub cache_dir: PathBuf,
    pub cache_limit_mb: u32,
}

const CONFIG_FILE_NAME: &'static str = "octobuild.conf";

#[cfg(windows)]
const DEFAULT_CACHE_DIR: &'static str = "~/.octobuild";
#[cfg(unix)]
const DEFAULT_CACHE_DIR: &'static str = "~/.cache/octobuild";

const PARAM_HELPER_BIND: &'static str = "helper_bind";
const PARAM_COORDINATOR_BIND: &'static str = "coordinator_bind";
const PARAM_COORDINATOR: &'static str = "coordinator";
const PARAM_CACHE_LIMIT: &'static str = "cache_limit_mb";
const PARAM_CACHE_PATH: &'static str = "cache_path";
const PARAM_PROCESS_LIMIT: &'static str = "process_limit";

impl Config {
    pub fn new() -> Result<Self> {
        let local = get_local_config_path().and_then(|v| load_config(v).ok());
        let global = get_global_config_path().and_then(|v| load_config(v).ok());
        Config::load(&local, &global, false)
    }

    pub fn get_coordinator_addrs(&self) -> Result<Vec<SocketAddr>> {
        match &self.coordinator {
            &Some(ref url) => url
                .with_default_port(|url| match url.scheme() {
                    "http" => Ok(80),
                    _ => Err(()),
                }).and_then(|host| host.to_socket_addrs())
                .map(|iter| iter.collect()),
            &None => Ok(Vec::new()),
        }
    }

    pub fn defaults() -> Result<Self> {
        Config::load(&None, &None, true)
    }

    fn load(local: &Option<Yaml>, global: &Option<Yaml>, defaults: bool) -> Result<Self> {
        let cache_limit_mb =
            get_config(local, global, PARAM_CACHE_LIMIT, |v| v.as_i64().map(|v| v as u32)).unwrap_or(16 * 1024);
        let cache_path = match defaults {
            true => None,
            false => env::var("OCTOBUILD_CACHE")
                .ok()
                .and_then(|v| if v == "" { None } else { Some(v) }),
        }.or_else(|| get_config(local, global, PARAM_CACHE_PATH, |v| v.as_str().map(|v| v.to_string())))
        .unwrap_or(DEFAULT_CACHE_DIR.to_string());
        let process_limit = get_config(local, global, PARAM_PROCESS_LIMIT, |v| v.as_i64().map(|v| v as usize))
            .unwrap_or_else(|| num_cpus::get());
        let coordinator = get_config(local, global, PARAM_COORDINATOR, |v| match v.is_null() {
            true => None,
            false => v.as_str().and_then(|v| {
                Url::parse(v)
                    .map(|mut v| {
                        v.set_path("");
                        v
                    }).ok()
            }),
        });
        let helper_bind = get_config(local, global, PARAM_HELPER_BIND, |v| {
            v.as_str().and_then(|v| FromStr::from_str(v).ok())
        }).unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 0)));
        let coordinator_bind = get_config(local, global, PARAM_COORDINATOR_BIND, |v| {
            v.as_str().and_then(|v| FromStr::from_str(v).ok())
        }).unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 3000)));

        Ok(Config {
            process_limit: process_limit,
            cache_dir: try!(replace_home(&cache_path)),
            cache_limit_mb: cache_limit_mb,
            coordinator: coordinator,
            helper_bind: helper_bind,
            coordinator_bind: coordinator_bind,
        })
    }

    fn show(&self) {
        let mut content = String::new();

        let mut y = Hash::new();
        y.insert(
            Yaml::String(PARAM_PROCESS_LIMIT.to_string()),
            Yaml::Integer(self.process_limit as i64),
        );
        y.insert(
            Yaml::String(PARAM_CACHE_LIMIT.to_string()),
            Yaml::Integer(self.cache_limit_mb as i64),
        );
        y.insert(
            Yaml::String(PARAM_CACHE_PATH.to_string()),
            Yaml::String(self.cache_dir.to_str().unwrap().to_string()),
        );
        y.insert(
            Yaml::String(PARAM_COORDINATOR.to_string()),
            self.coordinator
                .as_ref()
                .map_or(Yaml::Null, |v| Yaml::String(v.as_str().to_string())),
        );
        y.insert(
            Yaml::String(PARAM_HELPER_BIND.to_string()),
            Yaml::String(self.helper_bind.to_string()),
        );
        y.insert(
            Yaml::String(PARAM_COORDINATOR_BIND.to_string()),
            Yaml::String(self.coordinator_bind.to_string()),
        );
        YamlEmitter::new(&mut content).dump(&Yaml::Hash(y)).unwrap();
        println!("{}", content);
    }

    pub fn help() {
        println!("Octobuild configuration:");
        println!(
            "  system config path: {}",
            get_global_config_path()
                .map(|v| v.to_str().unwrap().to_string())
                .unwrap_or("none".to_string())
        );
        println!(
            "  user config path:   {}",
            get_local_config_path()
                .map(|v| v.to_str().unwrap().to_string())
                .unwrap_or("none".to_string())
        );
        println!("");
        println!("Actual configuration:");
        match Config::new() {
            Ok(c) => {
                c.show();
            }
            Err(e) => {
                println!("  ERROR: {}", e.description());
            }
        }
        println!("");
        println!("Default configuration:");
        match Config::defaults() {
            Ok(c) => {
                c.show();
            }
            Err(e) => {
                println!("  ERROR: {}", e.description());
            }
        }
        println!("");
    }
}

fn get_config<F, T>(local: &Option<Yaml>, global: &Option<Yaml>, param: &str, op: F) -> Option<T>
where
    F: Fn(&Yaml) -> Option<T>,
{
    None.or_else(|| local.as_ref().and_then(|i| op(&i[param])))
        .or_else(|| global.as_ref().and_then(|i| op(&i[param])))
}

fn load_config<P: AsRef<Path>>(path: P) -> Result<Yaml> {
    let mut file = try!(File::open(path));
    let mut content = String::new();
    try!(file.read_to_string(&mut content));
    match YamlLoader::load_from_str(&content) {
        Ok(ref mut docs) => Ok(docs.pop().unwrap()),
        Err(e) => Err(io::Error::new(ErrorKind::InvalidInput, e)),
    }
}

fn get_local_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|v| v.join(&(".".to_string() + CONFIG_FILE_NAME)))
}

#[cfg(windows)]
fn get_global_config_path() -> Option<PathBuf> {
    env::var("ProgramData")
        .ok()
        .map(|v| Path::new(&v).join("octobuild").join(CONFIG_FILE_NAME))
}

#[cfg(unix)]
fn get_global_config_path() -> Option<PathBuf> {
    Some(Path::new("/etc/octobuild").join(CONFIG_FILE_NAME).to_path_buf())
}

fn replace_home(path: &str) -> Result<PathBuf> {
    if path.starts_with("~/") {
        dirs::home_dir()
            .map(|v| v.join(&path[2..]))
            .ok_or(io::Error::new(ErrorKind::NotFound, "Can't determinate user HOME path"))
    } else {
        Ok(Path::new(path).to_path_buf())
    }
}
