extern crate octobuild;
extern crate daemon;
extern crate iron;
extern crate router;
extern crate fern;
extern crate time;
extern crate rustc_serialize;
#[macro_use]
extern crate nickel;
#[macro_use]
extern crate log;

use octobuild::config::Config;
use octobuild::cluster::common::{BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_LIST, RPC_BUILDER_UPDATE};
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use nickel::{HttpRouter, MediaType, Middleware, MiddlewareResult, Nickel, Request, Response};
use nickel::status::StatusCode;
use rustc_serialize::json;
use time::{Duration, Timespec};
use std::io::Read;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};

struct BuilderState {
    pub guid: String,
    pub info: BuilderInfo,
    pub timeout: Timespec,
}

struct CoordinatorService {
    state: Arc<CoordinatorState>,
    http: Nickel,
}

struct CoordinatorState {
    builders: RwLock<Vec<BuilderState>>,
}

impl CoordinatorState {
    pub fn new() -> Self {
        CoordinatorState { builders: RwLock::new(Vec::new()) }
    }
}

impl CoordinatorService {
    pub fn new() -> Self {
        CoordinatorService {
            state: Arc::new(CoordinatorState::new()),
            http: Nickel::new(),
        }
    }
}

struct RpcAgentUpdateHandler(Arc<CoordinatorState>);
struct RpcAgentListHandler(Arc<CoordinatorState>);

impl<D> Middleware<D> for RpcAgentUpdateHandler {
    fn invoke<'a, 'server>(&'a self,
                           request: &mut Request<'a, 'server, D>,
                           mut response: Response<'a, D>)
                           -> MiddlewareResult<'a, D> {
        let mut payload = String::new();
        request.origin.read_to_string(&mut payload).unwrap();
        let update: BuilderInfoUpdate = json::decode(&payload).unwrap();
        {
            let mut holder = self.0.builders.write().unwrap();
            let now = time::get_time();
            holder.retain(|e| (e.guid != update.guid) && (e.timeout >= now));
            payload = json::encode(&update.info).unwrap();
            holder.push(BuilderState {
                guid: update.guid,
                info: update.info,
                timeout: now + Duration::seconds(5),
            });
        }

        response.set(StatusCode::Ok);
        response.set(MediaType::Json);
        response.send(payload)
    }
}

impl<D> Middleware<D> for RpcAgentListHandler {
    fn invoke<'a, 'server>(&'a self,
                           _: &mut Request<'a, 'server, D>,
                           mut response: Response<'a, D>)
                           -> MiddlewareResult<'a, D> {
        let holder = self.0.builders.read().unwrap();
        let now = time::get_time();
        let builders: Vec<&BuilderInfo> = holder.iter()
            .filter_map(|e| {
                match e.timeout >= now {
                    true => Some(&e.info),
                    false => None,
                }
            })
            .collect();

        response.set(StatusCode::Ok);
        response.set(MediaType::Json);
        response.send(json::encode(&builders).unwrap())
    }
}

fn main() {
    let daemon = Daemon { name: "octobuild_coordinator".to_string() };

    daemon.run(move |rx: Receiver<State>| {
            octobuild::utils::init_logger();

            info!("Coordinator started.");
            let mut web = None;
            for signal in rx.iter() {
                match signal {
                    State::Start => {
                        let config = Config::new().unwrap();
                        info!("Coordinator bind to address: {}", config.coordinator_bind);

                        let mut server = CoordinatorService::new();
                        server.http.get(RPC_BUILDER_LIST, RpcAgentListHandler(server.state.clone()));
                        server.http.post(RPC_BUILDER_UPDATE,
                                         RpcAgentUpdateHandler(server.state.clone()));

                        let listener = server.http.listen(config.coordinator_bind).unwrap();

                        web = Some(listener);
                        info!("Coordinator: Ready");
                    }
                    State::Reload => {
                        info!("Coordinator: Reload");
                    }
                    State::Stop => {
                        info!("Coordinator: Stoping");
                        match web.take() {
                            Some(mut v) => {
                                v.detach();
                            }
                            None => {}
                        }
                        info!("Coordinator: Stoped");
                    }
                };
            }
            info!("Coordinator shutdowned.");
        })
        .unwrap();
}
