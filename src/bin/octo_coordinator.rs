extern crate octobuild;
extern crate daemon;
extern crate iron;
extern crate router;
extern crate fern;
extern crate time;
extern crate rustc_serialize;
#[macro_use]
extern crate log;

use octobuild::cluster::common::{BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_LIST, RPC_BUILDER_UPDATE};
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use iron::prelude::*;
use iron::status;
use rustc_serialize::json;
use time::{Timespec, Duration};
use std::io::Read;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, Arc};

struct BuilderState {
    pub guid: String,
    pub info: BuilderInfo,
    pub timeout: Timespec,
}

struct CoordinatorService {
    pub builders: Mutex<Vec<BuilderState>>,
}

macro_rules! service_router {
    ($service:expr,$($method:ident $glob:expr => $handler:ident),+ $(,)*) => ({
        let mut router = router::Router::new();
        let service = Arc::new($service);
        $({
            let service_clone = service.clone();
            router.$method($glob, move |r: &mut Request| service_clone.$handler(r) );
        })*
        router
    });
}

impl CoordinatorService {
    pub fn new() -> CoordinatorService {
        CoordinatorService {
            builders: Mutex::new(Vec::new()),
        }
    }

    pub fn rpc_agent_list(&self, _: &mut Request) -> IronResult<Response> {
        let holder = self.builders.lock().unwrap();
        let now = time::get_time();
        let builders: Vec<&BuilderInfo> = holder.iter()
            .filter_map(|e| match e.timeout >= now {
                true => Some(&e.info),
                false => None,
            })
            .collect();
        Ok(Response::with((status::Ok, json::encode(&builders).unwrap())))
    }

    pub fn rpc_agent_update(&self, request: &mut Request) -> IronResult<Response> {
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        let update: BuilderInfoUpdate = json::decode(&payload).unwrap();
        {
            let mut holder = self.builders.lock().unwrap();
            let now = time::get_time();
            holder.retain(|e| (e.guid != update.guid) && (e.timeout >= now));
            payload = json::encode(&update.info).unwrap();
            holder.push(BuilderState{
                guid: update.guid,
                info: update.info,
                timeout: now + Duration::seconds(5),
            });
        }
        Ok(Response::with((status::Ok, payload)))
    }
}

fn main() {
    let daemon = Daemon {
        name: "octobuild_coordinator".to_string()
    };

    daemon.run(move |rx: Receiver<State>| {
        octobuild::utils::init_logger();

        info!("Coordinator started.");
        let mut web = None;
        for signal in rx.iter() {
            match signal {
                State::Start => {
                    info!("Coordinator: Starting on 3000");
                    let router = service_router!(CoordinatorService::new(),
                        get RPC_BUILDER_LIST => rpc_agent_list,
                        post RPC_BUILDER_UPDATE => rpc_agent_update,
                    );
                    web = Some(Iron::new(router).http("localhost:3000").unwrap());
                    info!("Coordinator: Readly");
                },
                State::Reload => {
                    info!("Coordinator: Reload");
                }
                State::Stop => {
                    info!("Coordinator: Stoping");
                    match web.take() {
                        Some(mut v) => { v.close().unwrap(); }
                        None => {}
                    }
                    info!("Coordinator: Stoped");
                }
            };
        }
        info!("Coordinator shutdowned.");
    }).unwrap();
}
