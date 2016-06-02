extern crate octobuild;
extern crate daemon;
extern crate router;
extern crate fern;
extern crate hyper;
extern crate mio;
extern crate rustc_serialize;
#[macro_use]
extern crate log;

use octobuild::cluster::common::{BuilderInfo, BuilderInfoUpdate};
use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use hyper::Client;
use mio::tcp::TcpListener;
use mio::util::Slab;
use rustc_serialize::json;
use std::error::Error;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc};
use std::str::FromStr;
use std::time::Duration;
use std::thread;
use std::thread::JoinHandle;

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
        let mut service = AgentService {
            done: Arc::new(AtomicBool::new(false)),
            sock: sock,
            annoncer: None,
        };
        let info = BuilderInfoUpdate::new(BuilderInfo {
            name: get_name(),
            endpoints: vec!(endpoint),
        });
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
            Some(t) => { t.join().unwrap(); },
            None => {},
        }
        println!("drop end");
    }
}

fn get_name() -> String {
    octobuild::hostname::get_host_name().unwrap()
}

fn main() {
    let daemon = Daemon {
        name: "octobuild_agent".to_string()
    };

    daemon.run(move |rx: Receiver<State>| {
        octobuild::utils::init_logger();

        info!("Agent started.");
        let mut agent = None;
        for signal in rx.iter() {
            match signal {
                State::Start => {
                    info!("Agent: Starting");
                    agent = Some(AgentService::new());
                    info!("Agent: Readly");
                },
                State::Reload => {
                    info!("Agent: Reload");
                }
                State::Stop => {
                    info!("Agent: Stoping");
                    agent.take();
                    info!("Agent: Stoped");
                }
            };
        }
        info!("Agent shutdowned.");
    }).unwrap();
}
