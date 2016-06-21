extern crate octobuild;
extern crate capnp;
extern crate daemon;
extern crate router;
extern crate fern;
extern crate hyper;
extern crate rustc_serialize;
extern crate tempdir;
#[macro_use]
extern crate nickel;
#[macro_use]
extern crate log;
extern crate md5;

use octobuild::config::Config;
use octobuild::compiler::*;
use octobuild::cluster::builder::{CompileRequest, CompileResponse};
use octobuild::cluster::common::{BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_TASK, RPC_BUILDER_UPDATE,
                                 RPC_BUILDER_UPLOAD};
use octobuild::version;
use octobuild::vs::compiler::VsCompiler;
use octobuild::clang::compiler::ClangCompiler;
use octobuild::io::memstream::MemStream;
use octobuild::io::tempfile::TempFile;
use octobuild::utils::DEFAULT_BUF_SIZE;
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use hyper::{Client, Url};
use md5::{Context, Digest};
use nickel::{HttpRouter, ListeningServer, MediaType, Middleware, MiddlewareResult, Nickel, NickelError, Request,
             Response};
use nickel::status::StatusCode;
use nickel::method::Method;
use rustc_serialize::json;
use rustc_serialize::hex::{FromHex, ToHex};
use tempdir::TempDir;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Write};
use std::iter::FromIterator;
use std::mem;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;
use std::thread::JoinHandle;

use capnp::message;

struct BuilderService {
    done: Arc<AtomicBool>,
    listener: Option<ListeningServer>,
    anoncer: Option<JoinHandle<()>>,
}

struct BuilderState {
    name: String,
    temp_dir: TempDir,
    precompiled_dir: PathBuf,
    toolchains: HashMap<String, Arc<Toolchain>>,
}

const PRECOMPILED_SUFFIX: &'static str = ".pch";

struct RpcBuilderTaskHandler(Arc<BuilderState>);
struct RpcBuilderUploadHandler(Arc<BuilderState>);

impl BuilderService {
    fn new() -> Self {
        let config = Config::new().unwrap();
        info!("Helper bind to address: {}", config.helper_bind);

        let temp_dir = TempDir::new("octobuild").ok().expect("Can't create temporary directory");
        let state = Arc::new(BuilderState {
            name: get_name(),
            toolchains: BuilderService::discovery_toolchains(temp_dir.path()),
            temp_dir: temp_dir,
            precompiled_dir: config.cache_dir,
        });

        let mut http = Nickel::new();
        http.add_route(Method::Head,
                       RPC_BUILDER_UPLOAD.to_string() + "/:hash",
                       RpcBuilderUploadHandler(state.clone()));
        http.post(RPC_BUILDER_UPLOAD.to_string() + "/:hash",
                  RpcBuilderUploadHandler(state.clone()));
        http.post(RPC_BUILDER_TASK, RpcBuilderTaskHandler(state.clone()));

        let listener = http.listen(config.helper_bind).unwrap();
        info!("Helper local address: {}", listener.socket());

        info!("Found toolchains:");
        for toolchain in state.toolchain_names().iter() {
            info!("- {}", toolchain);
        }

        let done = Arc::new(AtomicBool::new(false));
        BuilderService {
            anoncer: Some(BuilderService::thread_anoncer(state.clone(),
                                                         config.coordinator.unwrap(),
                                                         done.clone(),
                                                         listener.socket())),
            done: done,
            listener: Some(listener),
        }
    }

    fn thread_anoncer(state: Arc<BuilderState>,
                      coordinator: Url,
                      done: Arc<AtomicBool>,
                      endpoint: SocketAddr)
                      -> JoinHandle<()> {
        thread::spawn(move || {
            let info = BuilderInfoUpdate::new(BuilderInfo {
                name: state.name.clone(),
                version: version::short_version(),
                endpoint: endpoint.to_string(),
                toolchains: state.toolchain_names(),
            });

            let client = Client::new();
            while !done.load(Ordering::Relaxed) {
                match client.post(coordinator.join(RPC_BUILDER_UPDATE).unwrap())
                    .body(&json::encode(&info).unwrap())
                    .send() {
                    Ok(_) => {}
                    Err(e) => {
                        info!("Builder: can't send info to coordinator: {}",
                              e.description());
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
    }

    fn discovery_toolchains(temp_dir: &Path) -> HashMap<String, Arc<Toolchain>> {
        let compiler = CompilerGroup::new(vec!(
			Box::new(VsCompiler::new(temp_dir)),
			Box::new(ClangCompiler::new()),
		));
        HashMap::from_iter(compiler.discovery_toolchains()
            .into_iter()
            .filter_map(|toolchain| toolchain.identifier().map(|name| (name, toolchain))))
    }
}

struct ReadWrapper<'a, R: 'a + Read>(&'a mut R);

impl<'a, R: 'a + Read> Read for ReadWrapper<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.0.read(buf)
    }
}

impl<D> Middleware<D> for RpcBuilderTaskHandler {
    fn invoke<'a, 'server>(&'a self,
                           req: &mut Request<'a, 'server, D>,
                           mut res: Response<'a, D>)
                           -> MiddlewareResult<'a, D> {
        let state = self.0.as_ref();
        // Receive compilation request.
        {
            info!("Received task from: {}", req.origin.remote_addr);
            let mut buf = BufReader::new(ReadWrapper(&mut req.origin));
            let mut options = ::capnp::message::ReaderOptions::new();
            options.traversal_limit_in_words(1024 * 1024 * 1024);
            let request = CompileRequest::stream_read(&mut buf, options).unwrap();
            let precompiled: Option<PathBuf> = match request.precompiled_hash {
                Some(ref hash) => {
                    if hash.len() != mem::size_of::<Digest>() {
                        return Err(NickelError::new(res,
                                                    format!("Invalid hash value: {}", hash.to_hex()),
                                                    StatusCode::BadRequest));
                    }
                    let path = state.precompiled_dir.join(hash.to_hex() + PRECOMPILED_SUFFIX);
                    if !path.exists() {
                        return Err(NickelError::new(res,
                                                    format!("Precompiled file not found: {}", hash.to_hex()),
                                                    StatusCode::BadRequest));
                    }
                    Some(path)
                }
                None => None,
            };
            let compile_step: CompileStep = CompileStep {
                output_object: None,
                output_precompiled: None,
                input_precompiled: precompiled,
                args: request.args,
                preprocessed: MemStream::from(request.preprocessed_data),
            };

            let toolchain: Arc<Toolchain> = state.toolchains.get(&request.toolchain).unwrap().clone();
            let response = CompileResponse::from(toolchain.compile_memory(compile_step));

            let mut payload = Vec::new();
            response.stream_write(&mut payload, &mut message::Builder::new_default()).unwrap();

            res.set(StatusCode::Ok);
            res.set(MediaType::Bin);
            res.send(payload)
        }
    }
}

impl<D> Middleware<D> for RpcBuilderUploadHandler {
    fn invoke<'a, 'server>(&'a self,
                           request: &mut Request<'a, 'server, D>,
                           mut response: Response<'a, D>)
                           -> MiddlewareResult<'a, D> {
        let state = self.0.as_ref();
        // Receive compilation request.
        let hash_str = match request.param("hash") {
            Some(v) => v.to_string(),
            None => {
                return Err(NickelError::new(response, "Hash is not defined", StatusCode::BadRequest));
            }
        };
        let hash = match hash_str.from_hex() {
            Ok(ref v) if v.len() == mem::size_of::<Digest>() => v.clone(),
            _ => {
                return Err(NickelError::new(response,
                                            format!("Invalid hash value: {}", hash_str),
                                            StatusCode::BadRequest));
            }
        };

        info!("Received upload from ({}): {} ",
              hash_str,
              request.origin.remote_addr);

        let path = state.precompiled_dir.join(hash.to_hex() + PRECOMPILED_SUFFIX);
        if path.exists() {
            // File is already uploaded
            response.set(StatusCode::Accepted);
            return response.send("");
        }

        // Upload file.
        let tempory = TempFile::new_in(&state.precompiled_dir, ".tmp");
        let mut hasher = Context::new();
        let mut temp = match File::create(tempory.path()) {
            Ok(f) => f,
            Err(e) => {
                return Err(NickelError::new(response,
                                            format!("Can't create file: {}", e),
                                            StatusCode::InternalServerError));
            }
        };
        let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
        loop {
            let size = match request.origin.read(&mut buf) {
                Ok(v) => v,
                Err(e) => {
                    return Err(NickelError::new(response,
                                                format!("Can't parse request body: {}", e),
                                                StatusCode::InternalServerError));
                }
            };
            if size <= 0 {
                break;
            }
            match temp.write(&buf[0..size]) {
                Ok(_) => {}
                Err(e) => {
                    return Err(NickelError::new(response,
                                                format!("Can't write file: {}", e),
                                                StatusCode::InternalServerError));
                }
            }
            hasher.consume(&buf[0..size]);
        }
        if hasher.compute().to_vec() != hash {
            return Err(NickelError::new(response, "Content hash mismatch", StatusCode::BadRequest));
        }
        drop(temp);

        match fs::rename(tempory.path(), &path) {
            Ok(_) => {}
            Err(e) => {
                if !path.exists() {
                    return Err(NickelError::new(response,
                                                format!("Can't rename file: {}", e),
                                                StatusCode::InternalServerError));
                }
            }
        }

        response.set(StatusCode::Ok);
        response.send("")
    }
}

impl BuilderState {
    fn toolchain_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.toolchains.keys().map(|s| s.clone()).collect();
        names.sort();
        names
    }
}

impl Drop for BuilderService {
    fn drop(&mut self) {
        println!("drop begin");
        self.done.store(true, Ordering::Relaxed);
        match self.anoncer.take() {
            Some(t) => {
                t.join().unwrap();
            }
            None => {}
        }
        match self.listener.take() {
            Some(t) => {
                t.detach();
            }
            None => {}
        }
        println!("drop end");
    }
}

fn get_name() -> String {
    octobuild::hostname::get_host_name().unwrap()
}

fn main() {
    let daemon = Daemon { name: "octobuild_Builder".to_string() };

    daemon.run(move |rx: Receiver<State>| {
            octobuild::utils::init_logger();

            info!("Builder started.");
            let mut builder = None;
            for signal in rx.iter() {
                match signal {
                    State::Start => {
                        info!("Builder: Starting");
                        builder = Some(BuilderService::new());
                        info!("Builder: Ready");
                    }
                    State::Reload => {
                        info!("Builder: Reload");
                    }
                    State::Stop => {
                        info!("Builder: Stoping");
                        builder.take();
                        info!("Builder: Stoped");
                    }
                };
            }
            info!("Builder shutdowned.");
        })
        .unwrap();
}
