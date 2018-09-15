use cluster::client::RemoteCompiler;
use compiler::*;
use config::Config;

use clang::compiler::ClangCompiler;
use vs::compiler::VsCompiler;

use worker::execute_graph;
use worker::{BuildAction, BuildGraph, BuildResult, BuildTask};

use petgraph::Graph;
use tempdir::TempDir;

use std::env;
use std::io;
use std::io::{Error, Write};
use std::iter::FromIterator;
use std::path::Path;
use std::sync::Arc;

pub fn supported_compilers(temp_dir: &Arc<TempDir>) -> CompilerGroup {
    CompilerGroup::new()
        .add(VsCompiler::new(&temp_dir))
        .add(ClangCompiler::new())
}

pub fn create_temp_dir() -> Result<Arc<TempDir>, Error> {
    TempDir::new("octobuild").map(|t| Arc::new(t))
}

pub fn simple_compile<C, F>(exec: &str, factory: F) -> i32
where
    C: Compiler,
    F: FnOnce(&Config) -> Result<C, Error>,
{
    let config = match Config::new() {
        Ok(v) => v,
        Err(e) => {
            error!("FATAL ERROR: Can't load configuration {}", e);
            return 501;
        }
    };
    let state = match SharedState::new(&config) {
        Ok(v) => v,
        Err(e) => {
            error!("FATAL ERROR: Can't create shared state {}", e);
            return 502;
        }
    };
    let compiler = match factory(&config) {
        Ok(v) => v,
        Err(e) => {
            error!("FATAL ERROR: Can't create compiler instance {}", e);
            return 503;
        }
    };
    match compile(&config, &state, exec, compiler) {
        Ok(status) => status.unwrap_or(503),
        Err(e) => {
            println!("FATAL ERROR: {}", e);
            500
        }
    }
}

pub fn compile<C>(config: &Config, state: &SharedState, exec: &str, compiler: C) -> Result<Option<i32>, Error>
where
    C: Compiler,
{
    let args = Vec::from_iter(env::args());
    let command_info = CommandInfo::simple(Path::new(exec));
    let remote = RemoteCompiler::new(&config.coordinator, compiler);
    let actions = BuildAction::create_tasks(&remote, command_info, &args[1..], exec);

    let mut build_graph: BuildGraph = Graph::new();
    for action in actions.into_iter() {
        build_graph.add_node(Arc::new(BuildTask {
            title: action.title().into_owned(),
            action,
        }));
    }
    let result = execute_graph(state, build_graph, config.process_limit, print_task_result);
    println!("{}", state.statistic.to_string());
    result
}

fn print_task_result(result: BuildResult) -> Result<(), Error> {
    match result.result {
        &Ok(ref output) => {
            io::stdout().write_all(&output.stdout)?;
            io::stderr().write_all(&output.stderr)?;
        }
        &Err(_) => {}
    }
    Ok(())
}
