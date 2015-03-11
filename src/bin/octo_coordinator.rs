#![feature(fs)]
#![feature(io)]
#![feature(env)]
#![feature(path)]

extern crate demon;
use demon::State;
use demon::Demon;
use demon::DemonRunner;
use std::env;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::Write;
use std::sync::mpsc::Receiver;

fn main() {
	log("Example started.");
	let demon = Demon {
		name: "octobuild_coordinator".to_string()
	};
	demon.run(move |rx: Receiver<State>| {
		log("Worker started.");
		for signal in rx.iter() {
			match signal {
				State::Start => log("Worker: Start"),
				State::Reload => log("Worker: Reload"),
				State::Stop => log("Worker: Stop")
			};
		}
		log("Worker finished.");
	}).unwrap();
	log("Example finished.");
}


#[allow(unused_must_use)]
fn log(message: &str) {
	log_safe(message);
}

fn log_safe(message: &str) -> Result<(), Error> {
//	println! ("{}", message);
	let path = try! (env::current_exe()).with_extension("log");
	let mut file = try! (OpenOptions::new().create(true).append(true).open(&path));
	try! (file.write(message.as_bytes()));
	try! (file.write(b"\n"));
	Ok(())
}
