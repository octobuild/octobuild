use std::env;
use std::io::Error;
use std::path::Path;
use std::sync::Arc;

use log::error;
use petgraph::Graph;
use tempdir::TempDir;

use crate::clang::compiler::ClangCompiler;
use crate::cluster::client::RemoteCompiler;
use crate::compiler::*;
use crate::config::Config;
use crate::vs::compiler::VsCompiler;
use crate::worker::execute_graph;
use crate::worker::{BuildAction, BuildGraph, BuildResult, BuildTask};

pub fn supported_compilers(temp_dir: &Arc<TempDir>) -> CompilerGroup {
    CompilerGroup::new()
        .add(VsCompiler::new(temp_dir))
        .add(ClangCompiler::new())
}

pub fn create_temp_dir() -> Result<Arc<TempDir>, Error> {
    TempDir::new("octobuild").map(Arc::new)
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

pub fn compile<C>(
    config: &Config,
    state: &SharedState,
    exec: &str,
    compiler: C,
) -> Result<Option<i32>, Error>
where
    C: Compiler,
{
    let args: Vec<String> = env::args().collect();
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
    println!("{}", state.statistic);
    result
}

fn print_task_result(result: BuildResult) -> Result<(), Error> {
    result.result.print_output()?;
    Ok(())
}
