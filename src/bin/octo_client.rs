extern crate octobuild;
extern crate capnp;
extern crate hyper;
extern crate rustc_serialize;
extern crate rand;
extern crate tempdir;
extern crate time;
#[macro_use]
extern crate log;

use octobuild::cluster::common::{BuilderInfo, RPC_BUILDER_LIST};
use octobuild::cluster::builder::{CompileRequest, CompileResponse};
use octobuild::compiler::{CompileStep, OutputInfo, Toolchain};

use hyper::{Client, Url};
use rustc_serialize::json;

use time::{Duration, Timespec};

use std::io::{BufReader, Error, ErrorKind, Read, Write};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::net::{SocketAddr, TcpStream};

use capnp::message;

struct RemoteCompiler {
    base_url: Url,
    cache: RwLock<RemoteCache>,
}

struct RemoteCache {
    cooldown: Timespec,
    builders: Arc<Vec<BuilderInfo>>,
}

struct RemoteToolchain {
    addr: SocketAddr,
    name: String,
}

impl RemoteCache {
    fn new() -> RemoteCache {
        RemoteCache {
            cooldown: Timespec { sec: 0, nsec: 0 },
            builders: Arc::new(Vec::new()),
        }
    }
}

impl RemoteCompiler {
    fn new<U: Into<Url>>(base_url: U) -> RemoteCompiler {
        RemoteCompiler {
            base_url: base_url.into(),
            cache: RwLock::new(RemoteCache::new()),
        }
    }

    fn builders(&self) -> Arc<Vec<BuilderInfo>> {
        let now = time::get_time();
        {
            let holder = self.cache.read().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
        }
        {
            let mut holder = self.cache.write().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
            match self.receive_builders() {
                Ok(builders) => {
                    holder.builders = Arc::new(builders);
                    holder.cooldown = now + Duration::seconds(5);
                }
                Err(e) => {
                    holder.cooldown = now + Duration::seconds(1);
                    warn!("Can't receive toolchains from coordinator: {}", e);
                }
            }
            return holder.builders.clone();
        }
    }

    fn receive_builders(&self) -> Result<Vec<BuilderInfo>, Error> {
        let client = Client::new();
        client.get(self.base_url.join(RPC_BUILDER_LIST).unwrap())
            .send()
            .map_err(|e| Error::new(ErrorKind::Other, e))
            .and_then(|mut response| {
                let mut payload = String::new();
                response.read_to_string(&mut payload).map(|_| payload)
            })
            .and_then(|payload| json::decode(&payload).map_err(|e| Error::new(ErrorKind::InvalidData, e)))
    }

    // Resolve toolchain for command execution.
    pub fn remote_toolchain(&self, toolchain_name: &str) -> Option<Arc<Toolchain>> {
        let name = toolchain_name.to_string();
        let all_builders = self.builders();
        get_random_builder(&all_builders, |b| b.toolchains.contains(&name))
            .and_then(|builder| SocketAddr::from_str(&builder.endpoint).ok())
            .map(|endpoint| -> Arc<Toolchain> {
                Arc::new(RemoteToolchain {
                    name: name,
                    addr: endpoint,
                })
            })
    }
}

impl Toolchain for RemoteToolchain {
    fn identifier(&self) -> Option<String> {
        Some(self.name.clone())
    }
    fn compile_step(&self, task: CompileStep) -> Result<OutputInfo, Error> {
        assert!(task.input_precompiled.is_none());
        assert!(task.output_object.is_none());
        assert!(task.output_precompiled.is_none());

        // Connect to builder.
        let mut ostream = try!(TcpStream::connect(self.addr));
        let mut istream = BufReader::new(ostream.try_clone().unwrap());

        // Send compilation request.
        let request = CompileRequest {
            toolchain: self.name.clone(),
            args: task.args.clone(),
            preprocessed: task.preprocessed.into(),
            precompiled: None,
        };
        try!(request.stream_write(&mut ostream, &mut message::Builder::new_default()));
        drop(request);

        // Receive compilation result.
        let response = CompileResponse::stream_read(&mut istream, ::capnp::message::ReaderOptions::new()).unwrap();
        match response {
            CompileResponse::Success(output) => Ok(output),
            CompileResponse::Err(err) => Err(err),
        }
    }
}

fn get_random_builder<F: Fn(&BuilderInfo) -> bool>(builders: &Vec<BuilderInfo>, filter: F) -> Option<&BuilderInfo> {
    let filtered: Vec<&BuilderInfo> = builders.iter().filter(|b| filter(b)).collect();
    if filtered.len() > 0 {
        Some(filtered[rand::random::<usize>() % filtered.len()].clone())
    } else {
        None
    }
}

fn main() {
    octobuild::utils::init_logger();

    let remote_compiler = RemoteCompiler::new(Url::parse("http://localhost:3000").unwrap());
    let all_builders = remote_compiler.builders();
    let builder = get_random_builder(&all_builders, |b| b.toolchains.len() > 0).unwrap();

    let toolchain = builder.toolchains.get(0).unwrap();

    info!("Builder: {}, {} ({})",
          builder.endpoint,
          builder.name,
          toolchain);
    let addr = SocketAddr::from_str(&builder.endpoint).unwrap();

    // Connect to builder.
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut buf = BufReader::new(stream.try_clone().unwrap());

    CompileRequest {
            toolchain: toolchain.clone(),
            args: vec!["-x".to_string(), "c++".to_string()],
            preprocessed: br#"
int main(int argc, char** argv) {
  return 0;
}
"#
                .to_vec(),
            precompiled: None,
        }
        .stream_write(&mut stream, &mut message::Builder::new_default())
        .unwrap();

    let response = CompileResponse::stream_read(&mut buf, ::capnp::message::ReaderOptions::new()).unwrap();
    info!("{:?}", response);

    let mut payload = String::new();
    stream.read_to_string(&mut payload).unwrap();
    info!("{}", payload);
}
