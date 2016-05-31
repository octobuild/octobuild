extern crate daemon;
extern crate iron;
extern crate fern;
extern crate time;
#[macro_use]
extern crate log;

use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use iron::prelude::*;
use iron::status;
use std::env;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::Receiver;

fn hello_world(_: &mut Request) -> IronResult<Response> {
    Ok(Response::with((status::Ok, "Hello World!")))
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
                    web = Some(Iron::new(hello_world).http("localhost:3000").unwrap());
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