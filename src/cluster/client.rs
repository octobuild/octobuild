use std::fs;
use std::fs::File;
use std::io::{Error, ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use log::{trace, warn};
use reqwest::blocking::Client;
use reqwest::StatusCode;

use crate::cache::FileHasher;
use crate::cluster::builder::{CompileRequest, CompileResponse};
use crate::cluster::common::{BuilderInfo, RPC_BUILDER_LIST, RPC_BUILDER_TASK, RPC_BUILDER_UPLOAD};
use crate::compiler::CompileInput::Preprocessed;
use crate::compiler::{
    CommandInfo, CompilationTask, CompileStep, Compiler, CompilerOutput, OutputInfo,
    PreprocessResult, SharedState, Toolchain,
};

pub struct RemoteCompiler<C: Compiler> {
    shared: Arc<RemoteShared>,
    local: C,
}

struct RemoteSharedMut {
    cooldown: Instant,
    #[allow(clippy::rc_buffer)]
    builders: Arc<Vec<BuilderInfo>>,
}

struct RemoteShared {
    mutable: RwLock<RemoteSharedMut>,
    base_url: Option<reqwest::Url>,
    client: Client,
}

struct RemoteToolchain {
    shared: Arc<RemoteShared>,
    local: Arc<dyn Toolchain>,
}

impl<C: Compiler> RemoteCompiler<C> {
    pub fn new(base_url: &Option<reqwest::Url>, compiler: C) -> Self {
        RemoteCompiler {
            shared: Arc::new(RemoteShared {
                mutable: RwLock::new(RemoteSharedMut {
                    cooldown: Instant::now(),
                    builders: Arc::new(Vec::new()),
                }),
                base_url: base_url.as_ref().cloned(),
                client: Client::new(),
            }),
            local: compiler,
        }
    }
}

impl RemoteSharedMut {
    fn receive_builders(base_url: &Option<reqwest::Url>) -> Result<Vec<BuilderInfo>, Error> {
        match base_url {
            Some(ref base_url) => {
                let url = base_url.join(RPC_BUILDER_LIST).unwrap();
                let mut response =
                    reqwest::blocking::get(url).map_err(|e| Error::new(ErrorKind::Other, e))?;

                bincode::deserialize_from(&mut response)
                    .map_err(|e| Error::new(ErrorKind::InvalidData, e))
            }
            None => Ok(Vec::new()),
        }
    }
}

impl<C: Compiler> Compiler for RemoteCompiler<C> {
    // Resolve toolchain for command execution.
    fn resolve_toolchain(&self, command: &CommandInfo) -> Option<Arc<dyn Toolchain>> {
        self.local
            .resolve_toolchain(command)
            .map(|local| -> Arc<dyn Toolchain> {
                Arc::new(RemoteToolchain {
                    shared: self.shared.clone(),
                    local,
                })
            })
    }

    // Discover local toolchains.
    fn discover_toolchains(&self) -> Vec<Arc<dyn Toolchain>> {
        self.local.discover_toolchains()
    }
}

struct ReadWrapper<'a, R: 'a + Read>(&'a mut R);

impl<'a, R: 'a + Read> Read for ReadWrapper<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.0.read(buf)
    }
}

impl RemoteToolchain {
    fn compile_remote(
        &self,
        state: &SharedState,
        task: &CompileStep,
    ) -> Result<CompileResponse, Error> {
        let name = self
            .identifier()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Can't get toolchain name"))?;

        let addr = self
            .remote_endpoint(&name)
            .ok_or_else(|| Error::new(ErrorKind::Other, "Can't find helper for toolchain"))?;
        if task.pch_usage.is_some() {
            return Err(Error::new(
                ErrorKind::Other,
                "Remote precompiled header generation is not supported",
            ));
        }

        let base_url = get_base_url(&addr);

        let preprocessed = if let Preprocessed(preprocessed) = &task.input {
            preprocessed
        } else {
            unimplemented!()
        };

        // Send compilation request.
        let request = CompileRequest {
            toolchain: name,
            args: task
                .args
                .iter()
                .map(|s| s.to_str().unwrap().to_string())
                .collect(),
            preprocessed_data: preprocessed.to_vec(),
            precompiled_hash: self.upload_precompiled(
                state,
                &task.pch_usage.get_in_abs(),
                &base_url,
            )?,
        };
        let request_payload = bincode::serialize(&request).unwrap();
        let mut resp: reqwest::blocking::Response = self
            .shared
            .client
            .post(base_url.join(RPC_BUILDER_TASK).unwrap())
            .body(request_payload)
            .send()
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
        // Receive compilation result.
        let result: CompileResponse = bincode::deserialize_from(&mut resp)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        if let CompileResponse::Success(ref output) = result {
            write_output(
                &task.output_object,
                output.success(),
                output.stdout.as_slice(),
            )?;
        }
        state.statistic.inc_remote();
        Ok(result)
    }

    fn upload_precompiled(
        &self,
        state: &SharedState,
        precompiled: &Option<&PathBuf>,
        base_url: &reqwest::Url,
    ) -> Result<Option<String>, Error> {
        match precompiled {
            Some(ref path) => {
                // Get precompiled header file hash
                let meta = state.cache.file_hash(path)?;
                // Check is precompiled header uploaded
                // todo: this is workaround for https://github.com/hyperium/hyper/issues/838
                match self
                    .shared
                    .client
                    .head(
                        base_url
                            .join(&format!("{RPC_BUILDER_UPLOAD}/{}", meta.hash))
                            .unwrap(),
                    )
                    .send()
                    .map(|response| response.status())
                    .map_err(|e| Error::new(ErrorKind::BrokenPipe, e))?
                {
                    StatusCode::OK | StatusCode::ACCEPTED => return Ok(Some(meta.hash)),
                    _ => {}
                }
                let file = File::open(path)?;
                // Upload precompiled header
                match self
                    .shared
                    .client
                    .post(
                        base_url
                            .join(&format!("{RPC_BUILDER_UPLOAD}/{}", meta.hash))
                            .unwrap(),
                    )
                    // todo: this is workaround for https://github.com/hyperium/hyper/issues/838
                    //.header(Expect::Continue)
                    .body(reqwest::blocking::Body::sized(file, meta.size))
                    .send()
                    .map(|response| response.status())
                    .map_err(|e| Error::new(ErrorKind::BrokenPipe, e))?
                {
                    StatusCode::OK | StatusCode::ACCEPTED => Ok(Some(meta.hash)),
                    status => Err(Error::new(
                        ErrorKind::BrokenPipe,
                        format!("Can't upload precompiled header: {status}"),
                    )),
                }
            }
            None => Ok(None),
        }
    }

    #[allow(clippy::rc_buffer)]
    fn builders(&self) -> Arc<Vec<BuilderInfo>> {
        let now = Instant::now();
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
            match RemoteSharedMut::receive_builders(&self.shared.base_url) {
                Ok(builders) => {
                    holder.builders = Arc::new(builders);
                    holder.cooldown = now + Duration::from_secs(5);
                }
                Err(e) => {
                    holder.cooldown = now + Duration::from_secs(1);
                    warn!("Can't receive toolchains from coordinator: {}", e);
                }
            }
            holder.builders.clone()
        }
    }
    // Resolve toolchain for command execution.
    fn remote_endpoint(&self, toolchain_name: &str) -> Option<SocketAddr> {
        let name = toolchain_name.to_string();
        let all_builders = self.builders();
        let builder = get_random_builder(&all_builders, |b| b.toolchains.contains(&name))?;
        SocketAddr::from_str(&builder.endpoint).ok()
    }
}

impl Toolchain for RemoteToolchain {
    fn identifier(&self) -> Option<String> {
        self.local.identifier()
    }

    // Parse compiler arguments.
    fn create_tasks(
        &self,
        command: CommandInfo,
        args: &[String],
        run_second_cpp: bool,
    ) -> crate::Result<Vec<CompilationTask>> {
        self.local.create_tasks(command, args, run_second_cpp)
    }

    // Preprocessing source file.
    fn run_preprocess(
        &self,
        state: &SharedState,
        task: &CompilationTask,
    ) -> crate::Result<PreprocessResult> {
        self.local.run_preprocess(state, task)
    }

    // Compile preprocessed file.
    fn create_compile_step(
        &self,
        task: &CompilationTask,
        preprocessed: CompilerOutput,
    ) -> crate::Result<CompileStep> {
        self.local.create_compile_step(task, preprocessed)
    }

    fn run_compile(&self, state: &SharedState, task: CompileStep) -> crate::Result<OutputInfo> {
        match self.compile_remote(state, &task) {
            Ok(response) => match response {
                CompileResponse::Success(output) => Ok(output),
                CompileResponse::Err(err) => Err(err.into()),
            },
            Err(e) => {
                trace!("Fallback to local build: {}", e);
                self.local.run_compile(state, task)
            }
        }
    }
}

fn get_base_url(addr: &SocketAddr) -> reqwest::Url {
    let mut url = reqwest::Url::from_str("http://localhost").unwrap();
    url.set_ip_host(addr.ip()).unwrap();
    url.set_port(Some(addr.port())).unwrap();
    url
}

fn write_output(path: &Option<PathBuf>, success: bool, output: &[u8]) -> Result<(), Error> {
    match path {
        Some(ref path) => {
            if success {
                let mut f = File::create(path)?;
                f.write(output).map_err(|e| {
                    drop(fs::remove_file(path));
                    e
                })?;
                Ok(())
            } else {
                fs::remove_file(path)
            }
        }
        None => Ok(()),
    }
}

fn get_random_builder<F: Fn(&BuilderInfo) -> bool>(
    builders: &[BuilderInfo],
    filter: F,
) -> Option<&BuilderInfo> {
    let filtered: Vec<&BuilderInfo> = builders.iter().filter(|b| filter(b)).collect();
    if filtered.is_empty() {
        return None;
    }

    Some(filtered[rand::random::<usize>() % filtered.len()])
}
