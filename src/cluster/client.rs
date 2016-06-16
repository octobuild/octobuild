use capnp::message;
use hyper::{Client, Url};
use rand;
use rustc_serialize::json;
use time;
use time::{Duration, Timespec};

use ::io::memstream::MemStream;
use ::cluster::common::{BuilderInfo, RPC_BUILDER_LIST};
use ::cluster::builder::{CompileRequest, CompileResponse, OptionalContent};
use ::compiler::{CommandInfo, CompilationTask, CompileStep, Compiler, OutputInfo, PreprocessResult, Toolchain};

use std::fs;
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind, Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::net::{SocketAddr, TcpStream};

pub struct RemoteCompiler<C: Compiler> {
    shared: Arc<RwLock<RemoteShared>>,
    local: C,
}

struct RemoteShared {
    base_url: Option<Url>,
    cooldown: Timespec,
    builders: Arc<Vec<BuilderInfo>>,
}

struct RemoteToolchain {
    shared: Arc<RwLock<RemoteShared>>,
    local: Arc<Toolchain>,
}

impl<C: Compiler> RemoteCompiler<C> {
    pub fn new(base_url: &Option<Url>, compiler: C) -> Self {
        RemoteCompiler {
            shared: Arc::new(RwLock::new(RemoteShared {
                cooldown: Timespec { sec: 0, nsec: 0 },
                builders: Arc::new(Vec::new()),
                base_url: base_url.as_ref().map(|u| u.clone()),
            })),
            local: compiler,
        }
    }
}

impl RemoteShared {
    fn receive_builders(&self) -> Result<Vec<BuilderInfo>, Error> {
        match &self.base_url {
            &Some(ref base_url) => {
                let client = Client::new();
                client.get(base_url.join(RPC_BUILDER_LIST).unwrap())
                    .send()
                    .map_err(|e| Error::new(ErrorKind::Other, e))
                    .and_then(|mut response| {
                        let mut payload = String::new();
                        response.read_to_string(&mut payload).map(|_| payload)
                    })
                    .and_then(|payload| json::decode(&payload).map_err(|e| Error::new(ErrorKind::InvalidData, e)))
            }
            &None => Ok(Vec::new()),
        }
    }
}

impl<C: Compiler> Compiler for RemoteCompiler<C> {
    // Discovery local toolchains.
    fn discovery_toolchains(&self) -> Vec<Arc<Toolchain>> {
        self.local.discovery_toolchains()
    }

    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<Toolchain>> {
        self.local
            .resolve_toolchain(command)
            .map(|local| -> Arc<Toolchain> {
                Arc::new(RemoteToolchain {
                    shared: self.shared.clone(),
                    local: local,
                })
            })
    }
}

impl RemoteToolchain {
    fn compile_remote(&self, task: &CompileStep) -> Result<CompileResponse, Error> {
        let name = try!(self.identifier().ok_or(Error::new(ErrorKind::Other, "Can't get toolchain name")));
        let addr = try!(self.remote_endpoint(&name)
            .ok_or(Error::new(ErrorKind::Other, "Can't find helper for toolchain")));
        if task.output_precompiled.is_some() {
            return Err(Error::new(ErrorKind::Other,
                                  "Remote precompiled header generation is not supported"));
        }

        let precompiled = match task.input_precompiled {
            Some(ref path) => {
                let content = try!(File::open(&path).and_then(|mut file| {
                    let mut buf = Vec::new();
                    file.read_to_end(&mut buf).map(|_| buf)
                }));
                Some(OptionalContent {
                    hash: "hash".to_string(), // todo: Need implementation
                    data: Some(content),
                })
            }
            None => None,
        };

        // Connect to builder.
        let mut ostream = try!(TcpStream::connect(addr));
        let mut istream = BufReader::new(ostream.try_clone().unwrap());

        // Send compilation request.
        let request = CompileRequest {
            toolchain: name.clone(),
            args: task.args.clone(),
            preprocessed: (&task.preprocessed).into(),
            precompiled: precompiled,
        };
        try!(request.stream_write(&mut ostream, &mut message::Builder::new_default()));
        drop(request);

        // Receive compilation result.
        let mut options = ::capnp::message::ReaderOptions::new();
        options.traversal_limit_in_words(1024 * 1024 * 1024);
        CompileResponse::stream_read(&mut istream, options)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
            .and_then(|result| {
                match result {
                    CompileResponse::Success(ref output, ref content) => {
                        try!(write_output(&task.output_object, output.success(), content));
                    }
                    _ => {}
                }
                Ok(result)
            })
    }

    fn builders(&self) -> Arc<Vec<BuilderInfo>> {
        let now = time::get_time();
        {
            let holder = self.shared.read().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
        }
        {
            let mut holder = self.shared.write().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
            match holder.receive_builders() {
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
    // Resolve toolchain for command execution.
    fn remote_endpoint(&self, toolchain_name: &str) -> Option<SocketAddr> {
        let name = toolchain_name.to_string();
        let all_builders = self.builders();
        get_random_builder(&all_builders, |b| b.toolchains.contains(&name))
            .and_then(|builder| SocketAddr::from_str(&builder.endpoint).ok())
    }
}

impl Toolchain for RemoteToolchain {
    fn identifier(&self) -> Option<String> {
        self.local.identifier()
    }

    // Parse compiler arguments.
    fn create_task(&self, command: CommandInfo, args: &[String]) -> Result<Option<CompilationTask>, String> {
        self.local.create_task(command, args)
    }

    // Preprocessing source file.
    fn preprocess_step(&self, task: &CompilationTask) -> Result<PreprocessResult, Error> {
        self.local.preprocess_step(task)
    }

    // Compile preprocessed file.
    fn compile_prepare_step(&self, task: CompilationTask, preprocessed: MemStream) -> Result<CompileStep, Error> {
        self.local.compile_prepare_step(task, preprocessed)
    }

    fn compile_step(&self, task: CompileStep) -> Result<OutputInfo, Error> {
        match self.compile_remote(&task) {
            Ok(response) => {
                match response {
                    CompileResponse::Success(output, _) => Ok(output),
                    CompileResponse::Err(err) => Err(err),
                }
            }
            Err(e) => {
                trace!("Fallback to local build: {}", e);
                self.local.compile_step(task)
            }
        }
    }
}

fn write_output(path: &Option<PathBuf>, success: bool, output: &[u8]) -> Result<(), Error> {
    match path {
        &Some(ref path) => {
            match success {
                true => {
                    File::create(path)
                        .and_then(|mut f| f.write(&output))
                        .or_else(|e| {
                            drop(fs::remove_file(path));
                            Err(e)
                        })
                        .map(|_| ())
                }
                false => fs::remove_file(path),
            }
        }
        &None => Ok(()),
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
