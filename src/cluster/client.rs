use capnp::message;
use hyper::{Client, Url};
use hyper::client::Body;
use hyper::header::Expect;
use hyper::status::StatusCode;
use rand;
use rustc_serialize::json;
use time;
use time::{Duration, Timespec};

use ::cache::{Cache, FileHasher};
use ::io::memstream::MemStream;
use ::io::statistic::Statistic;
use ::cluster::common::{BuilderInfo, RPC_BUILDER_LIST, RPC_BUILDER_TASK, RPC_BUILDER_UPLOAD};
use ::cluster::builder::{CompileRequest, CompileResponse};
use ::compiler::{CommandInfo, CompilationTask, CompileStep, Compiler, OutputInfo, PreprocessResult, Toolchain};

use std::fs;
use std::fs::File;
use std::io::{BufReader, Error, ErrorKind, Read, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::net::SocketAddr;

pub struct RemoteCompiler<C: Compiler> {
    shared: Arc<RemoteShared>,
    local: C,
}

struct RemoteSharedMut {
    cooldown: Timespec,
    builders: Arc<Vec<BuilderInfo>>,
}

struct RemoteShared {
    mutable: RwLock<RemoteSharedMut>,
    base_url: Option<Url>,
    statistic: Arc<Statistic>,
    cache: Arc<Cache>,
    client: Client,
}

struct RemoteToolchain {
    shared: Arc<RemoteShared>,
    local: Arc<Toolchain>,
}

impl<C: Compiler> RemoteCompiler<C> {
    pub fn new(base_url: &Option<Url>, compiler: C, cache: &Arc<Cache>, statistic: &Arc<Statistic>) -> Self {
        RemoteCompiler {
            shared: Arc::new(RemoteShared {
                mutable: RwLock::new(RemoteSharedMut {
                    cooldown: Timespec { sec: 0, nsec: 0 },
                    builders: Arc::new(Vec::new()),
                }),
                base_url: base_url.as_ref().map(|u| u.clone()),
                cache: cache.clone(),
                statistic: statistic.clone(),
                client: Client::new(),
            }),
            local: compiler,
        }
    }
}

impl RemoteSharedMut {
    fn receive_builders(&self, base_url: &Option<Url>) -> Result<Vec<BuilderInfo>, Error> {
        match base_url {
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

struct ReadWrapper<'a, R: 'a + Read>(&'a mut R);

impl<'a, R: 'a + Read> Read for ReadWrapper<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.0.read(buf)
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

        let base_url = get_base_url(&addr);
        // Send compilation request.
        let request = CompileRequest {
            toolchain: name.clone(),
            args: task.args.clone(),
            preprocessed_data: (&task.preprocessed).into(),
            precompiled_hash: try!(self.upload_precompiled(&task.input_precompiled, &base_url)),
        };
        let mut request_payload = Vec::new();
        try!(request.stream_write(&mut request_payload, &mut message::Builder::new_default()));
        self.shared
            .client
            .post(base_url.join(RPC_BUILDER_TASK)
                .unwrap())
            .body(&request_payload[..])
            .send()
            .map_err(|e| Error::new(ErrorKind::Other, e))
            .and_then(|mut response| {
                // Receive compilation result.
                let mut options = ::capnp::message::ReaderOptions::new();
                options.traversal_limit_in_words(1024 * 1024 * 1024);
                CompileResponse::stream_read(&mut BufReader::new(ReadWrapper(&mut response)), options)
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))
                    .and_then(|result| {
                        match result {
                            CompileResponse::Success(ref output, ref content) => {
                                try!(write_output(&task.output_object, output.success(), content));
                            }
                            _ => {}
                        }
                        self.shared.statistic.inc_remote();
                        Ok(result)
                    })
            })
    }

    fn upload_precompiled(&self, precompiled: &Option<PathBuf>, base_url: &Url) -> Result<Option<String>, Error> {
        match precompiled {
            &Some(ref path) => {
                // Get precompiled header file hash
                let meta = try!(self.shared.cache.file_hash(&path));
                // Check is precompiled header uploaded
                // todo: this is workaround for https://github.com/hyperium/hyper/issues/838
                match try!(self.shared
                    .client
                    .head(base_url.join(&format!("{}/{}", RPC_BUILDER_UPLOAD, meta.hash))
                        .unwrap())
                    .send()
                    .map(|response| response.status)
                    .map_err(|e| Error::new(ErrorKind::BrokenPipe, e))) {
                    StatusCode::Ok | StatusCode::Accepted => return Ok(Some(meta.hash)),
                    _ => {}
                }
                let mut file = try!(File::open(path));
                // Upload precompiled header
                match try!(self.shared
                    .client
                    .post(base_url.join(&format!("{}/{}", RPC_BUILDER_UPLOAD, meta.hash))
                        .unwrap())
                // todo: this is workaround for https://github.com/hyperium/hyper/issues/838
                    //.header(Expect::Continue)
                    .body(Body::SizedBody(&mut file, meta.size))
                    .send()
                    .map(|response| response.status)
                    .map_err(|e| Error::new(ErrorKind::BrokenPipe, e))) {
                    StatusCode::Ok | StatusCode::Accepted => Ok(Some(meta.hash)),
                    status => {
                        Err(Error::new(ErrorKind::BrokenPipe,
                                       format!("Can't upload precompiled header: {}", status)))
                    }
                }
            }
            &None => Ok(None),
        }
    }

    fn builders(&self) -> Arc<Vec<BuilderInfo>> {
        let now = time::get_time();
        {
            let holder = self.shared.mutable.read().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
        }
        {
            let mut holder = self.shared.mutable.write().unwrap();
            if holder.cooldown >= now {
                return holder.builders.clone();
            }
            match holder.receive_builders(&self.shared.base_url) {
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
    fn create_tasks(&self, command: CommandInfo, args: &[String]) -> Result<Vec<CompilationTask>, String> {
        self.local.create_tasks(command, args)
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

fn get_base_url(addr: &SocketAddr) -> Url {
    let mut url = Url::from_str("http://localhost").unwrap();
    url.set_ip_host(addr.ip()).unwrap();
    url.set_port(Some(addr.port())).unwrap();
    url
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
