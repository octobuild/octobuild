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
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use hyper::{Client, Url};
use nickel::{HttpRouter, ListeningServer, MediaType, Middleware, MiddlewareResult, Nickel, NickelError, Request,
             Response};
use nickel::status::StatusCode;
use rustc_serialize::json;
use tempdir::TempDir;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Write};
use std::iter::FromIterator;
use std::net::SocketAddr;
use std::path::Path;
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
    toolchains: HashMap<String, Arc<Toolchain>>,
}

struct RpcBuilderTaskHandler(Arc<BuilderState>);

impl BuilderService {
    fn new() -> Self {
        let config = Config::new().unwrap();
        info!("Helper bind to address: {}", config.helper_bind);

        let temp_dir = TempDir::new("octobuild").ok().expect("Can't create temporary directory");
        let state = Arc::new(BuilderState {
            name: get_name(),
            toolchains: BuilderService::discovery_toolchains(temp_dir.path()),
            temp_dir: temp_dir,
        });

        let mut http = Nickel::new();
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
            let precompiled: Option<TempFile> = match request.precompiled {
                Some(ref content) => {
                    let temp = TempFile::new_in(state.temp_dir.path(), ".pch");
                    match File::create(temp.path()).and_then(|mut file| file.write(&content.data.as_ref().unwrap())) {
                        Ok(_) => Some(temp),
                        Err(e) => {
                            return Err(NickelError::new(res,
                                                        format!("Internal server error: {}", e),
                                                        StatusCode::InternalServerError));
                        }
                    }
                }
                None => None,
            };
            let compile_step: CompileStep = CompileStep {
                output_object: None,
                output_precompiled: None,
                input_precompiled: precompiled.as_ref().map(|temp| temp.path().to_path_buf()),
                args: request.args,
                preprocessed: MemStream::from(request.preprocessed),
            };

            let toolchain: Arc<Toolchain> = state.toolchains.get(&request.toolchain).unwrap().clone();
            let response = CompileResponse::from(toolchain.compile_memory(compile_step));
            drop(precompiled);

            let mut payload = Vec::new();
            response.stream_write(&mut payload, &mut message::Builder::new_default()).unwrap();

            res.set(StatusCode::Ok);
            res.set(MediaType::Bin);
            res.send(payload)
        }
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
