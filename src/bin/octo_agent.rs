extern crate daemon;
extern crate iron;
extern crate router;
extern crate fern;
extern crate hyper;
extern crate mio;
extern crate time;
extern crate uuid;
extern crate rustc_serialize;
#[macro_use]
extern crate log;

use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use iron::prelude::*;
use iron::status;
use hyper::Client;
use mio::tcp::TcpListener;
use mio::util::Slab;
use rustc_serialize::{Decodable, Decoder, Encodable, Encoder};
use rustc_serialize::json;
use time::Timespec;
use uuid::Uuid;
use std::error::Error;
use std::io;
use std::io::{ErrorKind, Read};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Arc};
use std::str::FromStr;
use std::time::Duration;
use std::thread;
use std::thread::{JoinHandle, Thread};

#[derive(RustcEncodable, RustcDecodable)]
struct BuilderInfo {
    pub id: String,
    pub endpoints: Vec<String>,
    pub timeout: i64,
}

struct AgentService {
    done: Arc<AtomicBool>,
    sock: TcpListener,
    annoncer: Option<JoinHandle<()>>,
}

impl AgentService {
    fn new() -> AgentService {
        let addr: SocketAddr = FromStr::from_str("127.0.0.1:0").ok().expect("Failed to parse host:port string");
        let sock = TcpListener::bind(&addr).ok().expect("Failed to bind address");

        let endpoint = sock.local_addr().unwrap().to_string();
        println!("{}", endpoint);

        let mut service = AgentService {
            done: Arc::new(AtomicBool::new(false)),
            sock: sock,
            annoncer: None,
        };
        let info = BuilderInfo {
            id: Uuid::new_v4().to_string(),
            endpoints: vec!(endpoint),
            timeout: 0,
        };
        let done = service.done.clone();
        service.annoncer = Some(thread::spawn(move || {
            let client = Client::new();
            while !done.load(Ordering::Relaxed)
            {
                match client
                .post("http://localhost:3000/rpc/v1/agent/update")
                .body(&json::encode(&info).unwrap())
                .send()
                {
                    Ok(_) => {}
                    Err(e) => {
                        info!("Agent: can't send info to coordinator: {}", e.description());
                    }
                }
                thread::sleep(Duration::from_secs(1));
            }
        }));
        service
    }
}

impl Drop for AgentService {
    fn drop(&mut self)
    {
        println!("drop begin");
        self.done.store(true, Ordering::Relaxed);
        match self.annoncer.take() {
            Some(t) => { t.join(); },
            None => {},
        }
        println!("drop end");
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
                    web = Some(AgentService::new());
                    info!("Coordinator: Readly");
                },
                State::Reload => {
                    info!("Coordinator: Reload");
                }
                State::Stop => {
                    info!("Coordinator: Stoping");
                    web = None;
                    info!("Coordinator: Stoped");
                }
            };
        }
        info!("Coordinator shutdowned.");
    }).unwrap();
}

fn init_logger() {
    let log_file = Path::new("octo_agent.log").to_path_buf();

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