extern crate daemon;
extern crate iron;
extern crate router;
extern crate fern;
extern crate time;
extern crate rustc_serialize;
#[macro_use]
extern crate log;

use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use iron::prelude::*;
use iron::status;
use rustc_serialize::{Decoder, Encodable, Encoder};
use rustc_serialize::json;
use time::Duration;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, Arc};

#[derive(RustcEncodable, RustcDecodable)]
struct BuilderInfo {
    pub id: String,
    pub endpoints: Vec<String>,
    pub timeout: i64,
}

struct CoordinatorService {
    pub builders: Mutex<Vec<BuilderInfo>>,
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
        let mut builders: Vec<BuilderInfo> = Vec::new();
        builders.push(BuilderInfo {
            id: "builder-1".to_string(),
            endpoints: vec!("127.0.0.1:1234".to_string(), "[::1]:1234".to_string()),
            timeout: 0,
        });
        builders.push(BuilderInfo {
            id: "builder-2".to_string(),
            endpoints: vec!("127.0.0.2:1234".to_string(), "[::1]:1235".to_string()),
            timeout: 0,
        });
        CoordinatorService {
            builders: Mutex::new(builders),
        }
    }

    pub fn rpc_agent_list(&self, _: &mut Request) -> IronResult<Response> {
        let mut holder = self.builders.lock().unwrap();
        let builders: &mut Vec<BuilderInfo> = &mut holder;
        let now = time::get_time().sec;
        builders.retain(|e| e.timeout >= now);
        Ok(Response::with((status::Ok, json::encode(builders).unwrap())))
    }

    pub fn rpc_agent_update(&self, request: &mut Request) -> IronResult<Response> {
        let mut payload = String::new();
        request.body.read_to_string(&mut payload).unwrap();
        let mut builder: BuilderInfo = json::decode(&payload).unwrap();
        {
            let mut holder = self.builders.lock().unwrap();
            holder.retain(|e| e.id != builder.id);
            builder.timeout = (time::get_time() + Duration::seconds(5)).sec;
            payload = json::encode(&builder).unwrap();
            holder.push(builder);
        }
        Ok(Response::with((status::Ok, payload)))
    }
}

fn main() {
    let daemon = Daemon {
        name: "octobuild_coordinator".to_string()
    };

    daemon.run(move |rx: Receiver<State>| {
        init_logger();

        info!("Coordinator started.");
        let mut web = None;
        for signal in rx.iter() {
            match signal {
                State::Start => {
                    info!("Coordinator: Starting on 3000");
                    let router = service_router!(CoordinatorService::new(),
                        get "/rpc/v1/agent/list" => rpc_agent_list,
                        post "/rpc/v1/agent/update" => rpc_agent_update,
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

fn init_logger() {
    let log_file = Path::new("octo_coordinator.log").to_path_buf();

    // Create a basic logger configuration
    let logger_config = fern::DispatchConfig {
        format: Box::new(|msg, level, _location| {
            // This format just displays [{level}] {message}
            format!("{} [{}] {}", time::now().rfc3339(), level, msg)
        }),
        // Output to stdout and the log file in the temporary directory we made above to test
        output: vec![fern::OutputConfig::stdout(), fern::OutputConfig::file(&log_file)],
        // Only log messages Info and above
        level: log::LogLevelFilter::Info,
    };

    if let Err(e) = fern::init_global_logger(logger_config, log::LogLevelFilter::Trace) {
        panic!("Failed to initialize global logger: {}", e);
    }
}