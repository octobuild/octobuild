extern crate octobuild;
extern crate daemon;
extern crate fern;
extern crate time;
extern crate rustc_serialize;
extern crate tiny_http;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use octobuild::config::Config;
use octobuild::cluster::common::{BuilderInfo, BuilderInfoUpdate, RPC_BUILDER_LIST, RPC_BUILDER_UPDATE};
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use rustc_serialize::json;
use time::{Duration, Timespec};
use tiny_http::{Header, Request, Response};
use std::io::{Cursor, Error, Read};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};
use std::thread;

lazy_static! {
		static ref  CONTENT_TYPE_JSON :Header= Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
	}

struct BuilderState {
    pub guid: String,
    pub info: BuilderInfo,
    pub timeout: Timespec,
}

struct CoordinatorService {
    pub builders: RwLock<Vec<BuilderState>>,
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
    pub fn new() -> Self {
        CoordinatorService { builders: RwLock::new(Vec::new()) }
    }

    pub fn rpc_builder_list(&self, _: &Request) -> Result<Response<Cursor<Vec<u8>>>, Error> {
        let holder = self.builders.read().unwrap();
        let now = time::get_time();
        let builders: Vec<&BuilderInfo> = holder.iter()
            .filter_map(|e| {
                match e.timeout >= now {
                    true => Some(&e.info),
                    false => None,
                }
            })
            .collect();
        Ok(Response::from_data(json::encode(&builders).unwrap()).with_header(CONTENT_TYPE_JSON.clone()))
    }

    pub fn rpc_builder_update(&self, request: &mut Request) -> Result<Response<Cursor<Vec<u8>>>, Error> {
        let mut payload = String::new();
        try!(request.as_reader().read_to_string(&mut payload));
        let update: BuilderInfoUpdate = json::decode(&payload).unwrap();
        {
            let mut holder = self.builders.write().unwrap();
            let now = time::get_time();
            holder.retain(|e| (e.guid != update.guid) && (e.timeout >= now));
            payload = json::encode(&update.info).unwrap();
            holder.push(BuilderState {
                guid: update.guid,
                info: update.info,
                timeout: now + Duration::seconds(5),
            });
        }
        Ok(Response::from_data(payload).with_header(CONTENT_TYPE_JSON.clone()))
    }

    pub fn handle(&self, request: Request) -> Result<(), Error> {
        match request.url().split('?').next().unwrap() {
            RPC_BUILDER_LIST => handle_request(request, |request| self.rpc_builder_list(request)),
            RPC_BUILDER_UPDATE => handle_request(request, |request| self.rpc_builder_update(request)),
            _ => {
                let message = format!("404 not found: {}", request.url());
                request.respond(Response::from_string(message).with_status_code(404))
            }
        }
    }
}

fn handle_request<R, F>(mut request: Request, handler: F) -> Result<(), Error>
    where R: Read,
          F: FnOnce(&mut Request) -> Result<Response<R>, Error>
{
    match handler(&mut request) {
        Ok(response) => request.respond(response),
        Err(e) => request.respond(Response::from_string(format!("ERROR: {}", e)).with_status_code(500)),
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

                        let server = Arc::new(tiny_http::Server::http(config.coordinator_bind).unwrap());
                        // Create simplie thread pool
                        let service = Arc::new(CoordinatorService::new());
                        let mut handles = Vec::new();
                        for _ in 0..4 {
                            let server = server.clone();
                            let service_local = service.clone();
                            handles.push(thread::spawn(move || {
                                for request in server.incoming_requests() {
                                    service_local.handle(request).err().map(|e| warn!("Request error: {}", e));
                                }
                            }));
                        }
                        web = Some((server, handles));
                        info!("Coordinator: Ready");
                    }
                    State::Reload => {
                        info!("Coordinator: Reload");
                    }
                    State::Stop => {
                        info!("Coordinator: Stoping");
                        web.take();
                        info!("Coordinator: Stoped");
                    }
                };
            }
            info!("Coordinator shutdowned.");
        })
        .unwrap();
}
