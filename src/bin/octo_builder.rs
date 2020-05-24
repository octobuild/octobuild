use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::iter::FromIterator;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use daemon::Daemon;
use daemon::DaemonRunner;
use daemon::State;
use log::info;
use nickel::hyper::method::Method;
use nickel::status::StatusCode;
use nickel::{
    HttpRouter, ListeningServer, MediaType, Middleware, MiddlewareResult, Nickel, NickelError,
    Request, Response,
};
use sha2::digest::DynDigest;
use sha2::{Digest, Sha256};
use tempdir::TempDir;

use octobuild::cluster::builder::{CompileRequest, CompileResponse};
use octobuild::cluster::common::{
    BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_TASK, RPC_BUILDER_UPDATE, RPC_BUILDER_UPLOAD,
};
use octobuild::compiler::*;
use octobuild::config::Config;
use octobuild::io::memstream::MemStream;
use octobuild::io::tempfile::TempFile;
use octobuild::simple::create_temp_dir;
use octobuild::simple::supported_compilers;
use octobuild::utils::DEFAULT_BUF_SIZE;
use octobuild::version;

struct BuilderService {
    done: Arc<AtomicBool>,
    listener: Option<ListeningServer>,
    anoncer: Option<JoinHandle<()>>,
}

struct BuilderState {
    name: String,
    shared: SharedState,
    precompiled_dir: PathBuf,
    toolchains: HashMap<String, Arc<dyn Toolchain>>,
    precompiled: Mutex<HashMap<String, Arc<PrecompiledFile>>>,
}

struct PrecompiledFile {
    lock: Mutex<()>,
}

const PRECOMPILED_SUFFIX: &str = ".pch";

struct RpcBuilderTaskHandler(Arc<BuilderState>);

struct RpcBuilderUploadHandler(Arc<BuilderState>);

impl BuilderService {
    fn new() -> Self {
        let config = Config::new().unwrap();
        info!("Helper bind to address: {}", config.helper_bind);

        let temp_dir = create_temp_dir().expect("Can't create temporary directory");
        let state = Arc::new(BuilderState {
            name: get_name(),
            shared: SharedState::new(&config).unwrap(),
            toolchains: BuilderService::discovery_toolchains(&temp_dir),
            precompiled_dir: config.cache_dir,
            precompiled: Mutex::new(HashMap::new()),
        });

        let mut http = Nickel::new();
        http.add_route(
            Method::Head,
            RPC_BUILDER_UPLOAD.to_string() + "/:hash",
            RpcBuilderUploadHandler(state.clone()),
        );
        http.post(
            RPC_BUILDER_UPLOAD.to_string() + "/:hash",
            RpcBuilderUploadHandler(state.clone()),
        );
        http.post(RPC_BUILDER_TASK, RpcBuilderTaskHandler(state.clone()));

        let listener = http.listen(config.helper_bind).unwrap();
        info!("Helper local address: {}", listener.socket());

        info!("Found toolchains:");
        for toolchain in state.toolchain_names().iter() {
            info!("- {}", toolchain);
        }

        let done = Arc::new(AtomicBool::new(false));
        BuilderService {
            anoncer: Some(BuilderService::thread_anoncer(
                state,
                config.coordinator.unwrap(),
                done.clone(),
                listener.socket(),
            )),
            done,
            listener: Some(listener),
        }
    }

    fn thread_anoncer(
        state: Arc<BuilderState>,
        coordinator: reqwest::Url,
        done: Arc<AtomicBool>,
        endpoint: SocketAddr,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let info = BuilderInfoUpdate::new(BuilderInfo {
                name: state.name.clone(),
                version: version::VERSION.to_owned(),
                endpoint: endpoint.to_string(),
                toolchains: state.toolchain_names(),
            });

            let client = reqwest::blocking::Client::new();
            while !done.load(Ordering::Relaxed) {
                match client
                    .post(coordinator.join(RPC_BUILDER_UPDATE).unwrap())
                    .body(bincode::serialize(&info).unwrap())
                    .send()
                {
                    Ok(_) => {}
                    Err(e) => {
                        info!("Builder: can't send info to coordinator: {}", e);
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
    }

    fn discovery_toolchains(temp_dir: &Arc<TempDir>) -> HashMap<String, Arc<dyn Toolchain>> {
        let compiler = supported_compilers(temp_dir);
        HashMap::from_iter(
            compiler
                .discovery_toolchains()
                .into_iter()
                .filter_map(|toolchain| toolchain.identifier().map(|name| (name, toolchain))),
        )
    }
}

struct ReadWrapper<'a, R: 'a + Read>(&'a mut R);

impl<'a, R: 'a + Read> Read for ReadWrapper<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.0.read(buf)
    }
}

impl<D> Middleware<D> for RpcBuilderTaskHandler {
    fn invoke<'a, 'server>(
        &'a self,
        req: &mut Request<'a, 'server, D>,
        mut res: Response<'a, D>,
    ) -> MiddlewareResult<'a, D> {
        let state = self.0.as_ref();
        // Receive compilation request.
        {
            info!("Received task from: {}", req.origin.remote_addr);
            let request: CompileRequest = bincode::deserialize_from(&mut req.origin).unwrap();
            let precompiled: Option<PathBuf> = match request.precompiled_hash {
                Some(ref hash) => {
                    if !is_valid_sha256(hash) {
                        return Err(NickelError::new(
                            res,
                            format!("Invalid hash value: {}", hash),
                            StatusCode::BadRequest,
                        ));
                    }
                    let path = state
                        .precompiled_dir
                        .join(hash.to_string() + PRECOMPILED_SUFFIX);
                    if !path.exists() {
                        return Err(NickelError::new(
                            res,
                            format!("Precompiled file not found: {}", hash),
                            StatusCode::FailedDependency,
                        ));
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

            let toolchain: Arc<dyn Toolchain> =
                state.toolchains.get(&request.toolchain).unwrap().clone();
            let response =
                CompileResponse::from(toolchain.compile_memory(&state.shared, compile_step));
            let payload = bincode::serialize(&response).unwrap();
            res.set(StatusCode::Ok);
            res.set(MediaType::Bin);
            res.send(payload)
        }
    }
}

impl<D> Middleware<D> for RpcBuilderUploadHandler {
    fn invoke<'a, 'server>(
        &'a self,
        request: &mut Request<'a, 'server, D>,
        mut response: Response<'a, D>,
    ) -> MiddlewareResult<'a, D> {
        let state = self.0.as_ref();
        // Receive compilation request.
        let hash = match request.param("hash") {
            Some(v) => v.to_string(),
            None => {
                return Err(NickelError::new(
                    response,
                    "Hash is not defined",
                    StatusCode::BadRequest,
                ));
            }
        };
        if !is_valid_sha256(&hash) {
            return Err(NickelError::new(
                response,
                format!("Invalid hash value: {}", hash),
                StatusCode::BadRequest,
            ));
        }
        info!(
            "Received upload from ({}, {}): {} ",
            request.origin.method, hash, request.origin.remote_addr
        );

        let path = state
            .precompiled_dir
            .join(hash.clone() + PRECOMPILED_SUFFIX);
        if path.exists() {
            // File is already uploaded
            response.set(StatusCode::Accepted);
            return response.send("");
        }

        if request.origin.method == Method::Head {
            // File not uploaded.
            response.set(StatusCode::NotFound);
            return response.send("");
        }

        // Don't upload same file in multiple threads.
        let precompiled: Arc<PrecompiledFile> = state.get_precompiled(&hash);
        let lock = precompiled.lock.lock().unwrap();
        if path.exists() {
            // File is already uploaded
            response.set(StatusCode::Accepted);
            return response.send("");
        }

        // Receive uploading file.
        let tempory = TempFile::wrap(&path.with_extension("tmp"));
        let mut hasher = Sha256::new();
        let mut temp = match File::create(tempory.path()) {
            Ok(f) => f,
            Err(e) => {
                return Err(NickelError::new(
                    response,
                    format!("Can't create file: {}", e),
                    StatusCode::InternalServerError,
                ));
            }
        };
        let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
        let mut total_size = 0;
        loop {
            let size = match request.origin.read(&mut buf) {
                Ok(v) => v,
                Err(e) => {
                    return Err(NickelError::new(
                        response,
                        format!("Can't parse request body: {}", e),
                        StatusCode::InternalServerError,
                    ));
                }
            };
            if size == 0 {
                break;
            }
            total_size += size;
            match temp.write(&buf[0..size]) {
                Ok(_) => {}
                Err(e) => {
                    return Err(NickelError::new(
                        response,
                        format!("Can't write file: {}", e),
                        StatusCode::InternalServerError,
                    ));
                }
            }
            Digest::input(&mut hasher, &buf[0..size]);
        }
        if hex::encode(hasher.result()) != hash {
            return Err(NickelError::new(
                response,
                format!("Content hash mismatch: {}, {}", hash, total_size),
                StatusCode::BadRequest,
            ));
        }
        drop(temp);

        match fs::rename(tempory.path(), &path) {
            Ok(_) => {}
            Err(e) => {
                if !path.exists() {
                    return Err(NickelError::new(
                        response,
                        format!("Can't rename file: {}", e),
                        StatusCode::InternalServerError,
                    ));
                }
            }
        }
        drop(lock);
        response.set(StatusCode::Ok);
        response.send("")
    }
}

fn is_valid_sha256(hash: &str) -> bool {
    hex::decode(hash)
        .ok()
        .map_or(false, |v| v.len() == Sha256::new().output_size() * 2)
}

impl BuilderState {
    fn toolchain_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.toolchains.keys().cloned().collect();
        names.sort();
        names
    }

    fn get_precompiled(&self, hash: &str) -> Arc<PrecompiledFile> {
        self.precompiled
            .lock()
            .unwrap()
            .entry(hash.to_string())
            .or_insert_with(|| {
                Arc::new(PrecompiledFile {
                    lock: Mutex::new(()),
                })
            })
            .clone()
    }
}

impl Drop for BuilderService {
    fn drop(&mut self) {
        println!("drop begin");
        self.done.store(true, Ordering::Relaxed);
        if let Some(t) = self.anoncer.take() {
            t.join().unwrap();
        }
        if let Some(t) = self.listener.take() {
            t.detach();
        }
        println!("drop end");
    }
}

fn get_name() -> String {
    hostname::get().unwrap().into_string().unwrap()
}

fn main() {
    let daemon = Daemon {
        name: "octobuild_Builder".to_string(),
    };

    daemon
        .run(move |rx: Receiver<State>| {
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
