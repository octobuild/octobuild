extern crate octobuild;
extern crate petgraph;
extern crate tempdir;

use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::*;
use octobuild::cluster::client::RemoteCompiler;
use octobuild::config::Config;

use octobuild::worker::execute_graph;
use octobuild::worker::{BuildAction, BuildGraph, BuildResult, BuildTask};

use petgraph::Graph;
use tempdir::TempDir;

use std::env;
use std::io;
use std::io::{Error, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::sync::Arc;
use std::process;

fn main() {
    process::exit(match compile() {
        Ok(status) => status.unwrap_or(501),
        Err(e) => {
            println!("FATAL ERROR: {}", e);
            500
        }
    })
}

fn compile() -> Result<Option<i32>, Error> {
    let config = try!(Config::new());
    let args = Vec::from_iter(env::args());
    let state = Arc::new(SharedState::new(&config));
    let command_info = CommandInfo::simple(Path::new("cl.exe"));
    let compiler = RemoteCompiler::new(&config.coordinator,
                                       VsCompiler::new(&Arc::new(try!(TempDir::new("octobuild")))),
                                       &state);
    let actions = BuildAction::create_tasks(&compiler, command_info, &args[1..], "cl");

    let mut build_graph: BuildGraph = Graph::new();
    for action in actions.into_iter() {
        build_graph.add_node(Arc::new(BuildTask {
            title: "".to_string(),
            action: action,
        }));
    }
    let result = execute_graph(state.clone(),
                               build_graph,
                               config.process_limit,
                               print_task_result);
    println!("{}", state.statistic.to_string());
    result
}


fn print_task_result(result: BuildResult) -> Result<(), Error> {
    match result.result {
        &Ok(ref output) => {
            try!(io::stdout().write_all(&output.stdout));
            try!(io::stderr().write_all(&output.stderr));
        }
        &Err(_) => {}
    }
    Ok(())
}
