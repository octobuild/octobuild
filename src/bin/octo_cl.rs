#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;

use std::os;
use std::io::{TempDir, IoError, IoErrorKind, Command};

fn main() {
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};
	let compiler = VsCompiler::new(temp_dir.path());
	match compiler.compile(&os::args()[1..]) {
		Ok(_) => {Ok(())}
		Err(e) => {
			let mut command = Command::new("cl.exe");
			command.args(os::args()[1..].as_slice());
			match command.output() {
				Ok(output) => {
					println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
					println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
					Ok(())
				}
				Err(e) => {
					Err(e)
				}
			}
		}
	};
}
