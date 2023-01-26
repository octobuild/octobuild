use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use daemon::Daemon;
use daemon::DaemonRunner;
use daemon::State;
use log::info;
use nickel::status::StatusCode;
use nickel::{
    HttpRouter, MediaType, Middleware, MiddlewareResult, Nickel, NickelError, Request, Response,
};

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

struct RpcAgentUpdateHandler(Arc<CoordinatorState>);

struct RpcAgentListHandler(Arc<CoordinatorState>);

impl<D> Middleware<D> for RpcAgentUpdateHandler {
    fn invoke<'a>(
        &'a self,
        request: &mut Request<'a, '_, D>,
        mut response: Response<'a, D>,
    ) -> MiddlewareResult<'a, D> {
        let mut update: BuilderInfoUpdate = bincode::deserialize_from(&mut request.origin).unwrap();
        // Fix inspecified endpoint IP address.
        let endpoint = match SocketAddr::from_str(&update.info.endpoint) {
            Ok(v) => v,
            Err(e) => {
                return Err(NickelError::new(
                    response,
                    format!("Can't parse endpoint address: {e}"),
                    StatusCode::BadRequest,
                ));
            }
        };
        if is_unspecified(&endpoint.ip()) {
            update.info.endpoint =
                SocketAddr::new(request.origin.remote_addr.ip(), endpoint.port()).to_string();
        }

        let payload: Vec<u8>;
        // Update information.
        {
            let mut holder = self.0.builders.write().unwrap();
            let now = Instant::now();
            holder.retain(|e| (e.guid != update.guid) && (e.timeout >= now));
            payload = bincode::serialize(&update.info).unwrap();
            holder.push(BuilderState {
                guid: update.guid,
                info: update.info,
                timeout: now + Duration::from_secs(5),
            });
        }

        response.set(StatusCode::Ok);
        response.set(MediaType::Bin);
        response.send(payload)
    }
}

impl<D> Middleware<D> for RpcAgentListHandler {
    fn invoke<'a>(
        &'a self,
        _: &mut Request<'a, '_, D>,
        mut response: Response<'a, D>,
    ) -> MiddlewareResult<'a, D> {
        let holder = self.0.builders.read().unwrap();
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

        response.set(StatusCode::Ok);
        response.set(MediaType::Bin);
        response.send(bincode::serialize(&builders).unwrap())
    }
}

fn is_unspecified(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ref ip) => ip.octets() == [0, 0, 0, 0],
        IpAddr::V6(ref ip) => ip.is_unspecified(),
    }
}

fn main() {
    let daemon = Daemon {
        name: "octobuild_coordinator".to_string(),
    };

    daemon
        .run(move |rx: Receiver<State>| {
            octobuild::utils::init_logger();

            info!("Coordinator started.");
            let mut web = None;
            for signal in rx.iter() {
                match signal {
                    State::Start => {
                        let config = Config::load().unwrap();
                        info!("Coordinator bind to address: {}", config.coordinator_bind);

                        let state = Arc::new(CoordinatorState::new());
                        let mut http = Nickel::new();
                        http.get(RPC_BUILDER_LIST, RpcAgentListHandler(state.clone()));
                        http.post(RPC_BUILDER_UPDATE, RpcAgentUpdateHandler(state.clone()));

                        let listener = http.listen(config.coordinator_bind).unwrap();

                        web = Some(listener);
                        info!("Coordinator: Ready");
                    }
                    State::Reload => {
                        info!("Coordinator: Reload");
                    }
                    State::Stop => {
                        info!("Coordinator: Stoping");
                        if let Some(v) = web.take() {
                            v.detach();
                        }
                        info!("Coordinator: Stoped");
                    }
                };
            }
            info!("Coordinator shutdowned.");
        })
        .unwrap();
}
