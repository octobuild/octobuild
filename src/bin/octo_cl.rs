#![allow(unstable)]
extern crate octobuild;
extern crate log;

use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;
use octobuild::cache::Cache;

use std::os;
use std::io::{TempDir, Command};

fn main() {
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};
	let compiler = VsCompiler::new(&Cache::new(), temp_dir.path());
	match compiler.compile(&Command::new("cl.exe"), &os::args()[1..]) {
		Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
		}
		Err(e) => {
			panic!(e);
		}
	};
}
