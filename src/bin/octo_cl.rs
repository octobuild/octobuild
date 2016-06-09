extern crate octobuild;
extern crate tempdir;

use octobuild::vs::compiler::VsCompiler;
use octobuild::io::statistic::Statistic;
use octobuild::compiler::*;
use octobuild::cache::Cache;
use octobuild::config::Config;

use tempdir::TempDir;

use std::env;
use std::io;
use std::io::{Error, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::sync::RwLock;
use std::process;

fn main() {
    process::exit(match compile() {
        Ok(output) => {
            match output.status {
                Some(r) => r,
                None => 501,
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
    let temp_dir = try!(TempDir::new("octobuild"));
    let cache = Cache::new(&try!(Config::new()));
    let compiler = VsCompiler::new(temp_dir.path());
    let args = Vec::from_iter(env::args());
    let command_info = CommandInfo::simple(Path::new("cl.exe"));
    let output = try!(compiler.compile(command_info, &args[1..], &cache, &statistic));

    try!(io::stdout().write_all(&output.stdout));
    try!(io::stderr().write_all(&output.stderr));
    println!("{}", statistic.read().unwrap().to_string());
    Ok(output)
}
