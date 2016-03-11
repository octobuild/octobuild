extern crate octobuild;

use octobuild::io::statistic::Statistic;
use octobuild::clang::compiler::ClangCompiler;
use octobuild::compiler::*;
use octobuild::cache::Cache;
use octobuild::config::Config;

use std::collections::HashMap;
use std::env;
use std::io;
use std::io::{Error, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::sync::{Arc, RwLock};
use std::process;

fn main() {
	process::exit(match compile() {
		Ok(output) => {
			match output.status {
				Some(r) => r,
				None => 501
			}
		}
		Err(e) => {
			println!("FATAL ERROR: {:?}", e);
			500
		}
	})
}

fn compile() -> Result<OutputInfo, Error> {
	let statistic = RwLock::new(Statistic::new());
	let compiler = ClangCompiler::new(&Cache::new(&try! (Config::new())));
	let args = Vec::from_iter(env::args());
	let output = try! (compiler.compile(CommandInfo {
		program: Path::new("clang").to_path_buf(),
		current_dir: None,
		env: Arc::new(HashMap::new()),
	}, &args[1..], &statistic));

	try !(io::stdout().write_all(&output.stdout));
	try !(io::stderr().write_all(&output.stderr));
	println!("{}", statistic.read().unwrap().to_string());
	Ok(output)
}
