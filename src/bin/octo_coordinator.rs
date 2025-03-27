use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use daemon::Daemon;
use daemon::DaemonRunner;
use daemon::State;
use log::info;
use rouille::{router, try_or_400, Request, Response, Server};

use octobuild::cluster::common::{
    BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_LIST, RPC_BUILDER_UPDATE,
};
use octobuild::config::Config;

struct BuilderState {
    pub guid: String,
    pub info: BuilderInfo,
    pub timeout: Instant,
}

struct CoordinatorState {
    builders: RwLock<Vec<BuilderState>>,
}

impl CoordinatorState {
    pub fn new() -> Self {
        CoordinatorState {
            builders: RwLock::new(Vec::new()),
        }
    }
}

fn update(state: Arc<CoordinatorState>, request: &Request) -> octobuild::Result<Response> {
    let mut update: BuilderInfoUpdate =
        bincode::decode_from_std_read(&mut request.data().unwrap(), bincode::config::standard())?;
    // Fix inspecified endpoint IP address.
    let endpoint = match SocketAddr::from_str(&update.info.endpoint) {
        Ok(v) => v,
        Err(e) => {
            return Ok(
                Response::text(format!("Can't parse endpoint address: {e}")).with_status_code(400)
            );
        }
    };
    if endpoint.ip().is_unspecified() {
        update.info.endpoint =
            SocketAddr::new(request.remote_addr().ip(), endpoint.port()).to_string();
    }

    let payload: Vec<u8>;
    // Update information.
    {
        let mut holder = state.builders.write().unwrap();
        let now = Instant::now();
        holder.retain(|e| (e.guid != update.guid) && (e.timeout >= now));
        payload = bincode::encode_to_vec(&update.info, bincode::config::standard())?;
        holder.push(BuilderState {
            guid: update.guid,
            info: update.info,
            timeout: now + Duration::from_secs(5),
        });
    }

    Ok(Response::from_data("application/octet-stream", payload))
}

fn list(state: Arc<CoordinatorState>) -> octobuild::Result<Response> {
    let holder = state.builders.read().unwrap();
    let now = Instant::now();
    let builders: Vec<&BuilderInfo> = holder
        .iter()
        .filter_map(|e| {
            if e.timeout >= now {
                Some(&e.info)
            } else {
                None
            }
        })
        .collect();

    Ok(Response::from_data(
        "application/octet-stream",
        bincode::encode_to_vec(&builders, bincode::config::standard())?,
    ))
}

fn main() {
    env_logger::init();

    let daemon = Daemon {
        name: "octobuild_coordinator".to_string(),
    };

    daemon
        .run(move |rx: Receiver<State>| {
            octobuild::utils::init_logger();

            info!("Coordinator started.");
            let mut web = None;
            for signal in rx {
                match signal {
                    State::Start => {
                        let config = Config::load().unwrap();
                        info!("Coordinator bind to address: {}", config.coordinator_bind);

                        let state = Arc::new(CoordinatorState::new());
                        let server = Server::new(config.coordinator_bind, move |request| {
                            router!(request,
                                (GET) [RPC_BUILDER_LIST] => {
                                    try_or_400!(list(state.clone()))
                                },
                                (POST) [RPC_BUILDER_UPDATE] => {
                                    try_or_400!(update(state.clone(), request))
                                },
                                _ => Response::empty_404(),
                            )
                        })
                        .unwrap();

                        web = Some(server.stoppable());
                        info!("Coordinator: Ready");
                    }
                    State::Reload => {
                        info!("Coordinator: Reload");
                    }
                    State::Stop => {
                        info!("Coordinator: Stoping");
                        if let Some((handle, sender)) = web.take() {
                            sender.send(()).unwrap();
                            handle.join().unwrap();
                        }
                        info!("Coordinator: Stoped");
                    }
                };
            }
            info!("Coordinator shutdowned.");
        })
        .unwrap();
}
