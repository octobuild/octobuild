use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use daemon::Daemon;
use daemon::DaemonRunner;
use daemon::State;
use log::info;
use path_absolutize::Absolutize;
use rouille::{router, try_or_400, Request, Response, Server};
use sha2::digest::DynDigest;
use sha2::{Digest, Sha256};

use octobuild::cluster::builder::{CompileRequest, CompileResponse};
use octobuild::cluster::common::{
    BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_TASK, RPC_BUILDER_UPDATE, RPC_BUILDER_UPLOAD,
};
use octobuild::compiler::CompileInput::Preprocessed;
use octobuild::compiler::{
    CompileStep, Compiler, CompilerOutput, PCHArgs, PCHUsage, SharedState, Toolchain,
};
use octobuild::config::Config;
use octobuild::io::tempfile::TempFile;
use octobuild::simple::supported_compilers;
use octobuild::version;

struct BuilderService {
    done: Arc<AtomicBool>,
    server: Option<(JoinHandle<()>, mpsc::Sender<()>)>,
    announcer: Option<JoinHandle<()>>,
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

impl BuilderService {
    fn new() -> octobuild::Result<Self> {
        let config = Config::load()?;
        info!("Helper bind to address: {}", config.helper_bind);

        let state = Arc::new(BuilderState {
            name: hostname::get()?.into_string().unwrap(),
            shared: SharedState::new(&config)?,
            toolchains: BuilderService::discover_toolchains(),
            precompiled_dir: config.cache,
            precompiled: Mutex::new(HashMap::new()),
        });
        let worker_state = state.clone();

        let server = Server::new(config.helper_bind, move |request| {
            router!(request,
                (HEAD) [RPC_BUILDER_UPLOAD.to_string() + "/:hash"] => {
                    try_or_400!(handle_upload(worker_state.clone(), request))
                },
                (POST) [RPC_BUILDER_UPLOAD.to_string() + "/:hash"] => {
                    try_or_400!(handle_upload(worker_state.clone(), request))
                },
                (POST) [RPC_BUILDER_TASK] => {
                    try_or_400!(handle_task(worker_state.clone(), request))
                },
                _ => Response::empty_404(),
            )
        })
        .unwrap();

        info!("Helper local address: {}", server.server_addr());

        info!("Found toolchains:");
        for toolchain in &state.toolchain_names() {
            info!("- {}", toolchain);
        }

        let done = Arc::new(AtomicBool::new(false));
        Ok(BuilderService {
            announcer: Some(BuilderService::thread_announcer(
                state,
                config.coordinator.unwrap(),
                done.clone(),
                server.server_addr(),
            )),
            done,
            server: Some(server.stoppable()),
        })
    }

    fn thread_announcer(
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

    #[must_use]
    fn discover_toolchains() -> HashMap<String, Arc<dyn Toolchain>> {
        let compiler = supported_compilers();
        compiler
            .discover_toolchains()
            .into_iter()
            .filter_map(|toolchain| toolchain.identifier().map(|name| (name, toolchain)))
            .collect()
    }
}

fn handle_task(state: Arc<BuilderState>, request: &Request) -> octobuild::Result<Response> {
    // Receive compilation request.
    info!("Received task from: {}", &request.remote_addr());
    let request: CompileRequest = bincode::deserialize_from(request.data().unwrap())?;
    let pch_usage: PCHUsage = match request.precompiled_hash {
        Some(ref hash) => {
            if !is_valid_sha256(hash) {
                return Ok(
                    Response::text(format!("Invalid hash value: {hash}")).with_status_code(400)
                );
            }
            let path = state
                .precompiled_dir
                .join(hash.to_string() + PRECOMPILED_SUFFIX);
            if !path.exists() {
                return Ok(
                    Response::text(format!("Precompiled file not found: {hash}"))
                        .with_status_code(424),
                );
            }
            let path_abs = path.absolutize()?.to_path_buf();
            PCHUsage::In(PCHArgs {
                path,
                path_abs,
                marker: None,
            })
        }
        None => PCHUsage::None,
    };
    let compile_step = CompileStep {
        output_object: None,
        pch_usage,
        args: request.args.iter().map(OsString::from).collect(),
        input: Preprocessed(CompilerOutput::Vec(request.preprocessed_data)),
        run_second_cpp: false,
    };

    let toolchain: Arc<dyn Toolchain> = state.toolchains.get(&request.toolchain).unwrap().clone();
    let response = CompileResponse::from(toolchain.run_compile(&state.shared, compile_step));
    let payload = bincode::serialize(&response)?;
    Ok(Response::from_data("application/octet-stream", payload))
}

fn handle_upload(state: Arc<BuilderState>, request: &Request) -> octobuild::Result<Response> {
    // Receive compilation request.
    let hash = match request.get_param("hash") {
        Some(v) => v,
        None => {
            return Ok(Response::text("Hash is not defined").with_status_code(400));
        }
    };
    if !is_valid_sha256(&hash) {
        return Ok(Response::text(format!("Invalid hash value: {hash}")).with_status_code(400));
    }
    info!(
        "Received upload from ({}, {}): {} ",
        request.method(),
        hash,
        request.remote_addr()
    );

    let path = state
        .precompiled_dir
        .join(hash.clone() + PRECOMPILED_SUFFIX);
    if path.exists() {
        // File is already uploaded
        return Ok(Response::text("").with_status_code(202));
    }

    if request.method() == "HEAD" {
        // File not uploaded.
        return Ok(Response::text("").with_status_code(404));
    }

    // Don't upload same file in multiple threads.
    let precompiled: Arc<PrecompiledFile> = state.get_precompiled(&hash);
    let lock = precompiled.lock.lock().unwrap();
    if path.exists() {
        // File is already uploaded
        return Ok(Response::text("").with_status_code(202));
    }

    // Receive uploading file.
    let temporary = TempFile::wrap(&path.with_extension("tmp"));
    let mut hasher = Sha256::new();
    let temp = match File::create(temporary.path()) {
        Ok(f) => f,
        Err(e) => {
            return Ok(Response::text(format!("Can't create file: {e}")).with_status_code(500));
        }
    };

    let mut tee = tee::TeeReader::new(request.data().unwrap(), temp);
    let written = std::io::copy(&mut tee, &mut hasher)?;

    if hex::encode(hasher.finalize()) != hash {
        return Ok(
            Response::text(format!("Content hash mismatch: {hash}, {written}"))
                .with_status_code(400),
        );
    }

    match fs::rename(temporary.path(), &path) {
        Ok(_) => {}
        Err(e) => {
            if !path.exists() {
                return Ok(Response::text(format!("Can't rename file: {e}")).with_status_code(500));
            }
        }
    }
    drop(lock);

    Ok(Response::text(""))
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
        self.done.store(true, Ordering::Relaxed);
        if let Some(t) = self.announcer.take() {
            t.join().unwrap();
        }
        if let Some((handle, sender)) = self.server.take() {
            sender.send(()).unwrap();
            handle.join().unwrap();
        }
    }
}

fn main() {
    env_logger::init();
    
    let daemon = Daemon {
        name: "octobuild_Builder".to_string(),
    };

    daemon
        .run(move |rx: Receiver<State>| {
            octobuild::utils::init_logger();

            info!("Builder started.");
            let mut builder = None;
            for signal in rx {
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
