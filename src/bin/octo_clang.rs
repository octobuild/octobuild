extern crate octobuild;
extern crate hyper;

use hyper::Url;

use octobuild::io::statistic::Statistic;
use octobuild::clang::compiler::ClangCompiler;
use octobuild::compiler::*;
use octobuild::cluster::client::RemoteCompiler;
use octobuild::cache::Cache;
use octobuild::config::Config;

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
            println!("FATAL ERROR: {}", e);
            500
        }
    })
}

fn compile() -> Result<OutputInfo, Error> {
    let statistic = RwLock::new(Statistic::new());
    let cache = Cache::new(&try!(Config::new()));
    let args = Vec::from_iter(env::args());
    let command_info = CommandInfo::simple(Path::new("clang"));
    let compiler = RemoteCompiler::new(Url::parse("http://localhost:3000").unwrap(),
                                       ClangCompiler::new());
    let output = try!(compiler.compile(command_info, &args[1..], &cache, &statistic));

    try!(io::stdout().write_all(&output.stdout));
    try!(io::stderr().write_all(&output.stderr));
    println!("{}", statistic.read().unwrap().to_string());
    Ok(output)
}
