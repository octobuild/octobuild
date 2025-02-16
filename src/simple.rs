use std::env;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::sync::Arc;

use log::error;
use petgraph::Graph;

use crate::clang::compiler::ClangCompiler;
use crate::cluster::client::RemoteCompiler;
use crate::compiler::{CommandArgs, CommandInfo, Compiler, CompilerGroup, SharedState};
use crate::config::Config;
use crate::vs::compiler::VsCompiler;
use crate::worker::execute_graph;
use crate::worker::{BuildAction, BuildGraph, BuildResult, BuildTask};

#[must_use]
pub fn supported_compilers() -> CompilerGroup {
    CompilerGroup::new()
        .add::<VsCompiler>()
        .add::<ClangCompiler>()
}

pub fn simple_compile<C, F>(exec: &str, factory: F) -> i32
where
    C: Compiler,
    F: FnOnce(&Config) -> crate::Result<C>,
{
    let config = match Config::load() {
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
        Ok(_) => 0,
        Err(e) => {
            error!("FATAL ERROR: {e}");
            1
        }
    }
}

pub fn compile<C>(
    config: &Config,
    state: &SharedState,
    exec: &str,
    compiler: C,
) -> crate::Result<()>
where
    C: Compiler,
{
    let command_info = CommandInfo::simple(PathBuf::from(exec));
    let remote = RemoteCompiler::new(&config.coordinator, compiler);
    let args = env::args().skip(1).collect();
    let actions = BuildAction::create_tasks(
        &remote,
        command_info,
        CommandArgs::Vec(args),
        exec,
        config.run_second_cpp,
    );

    let mut build_graph: BuildGraph = Graph::new();
    for action in actions {
        build_graph.add_node(Arc::new(BuildTask {
            title: action.title().into_owned(),
            action,
        }));
    }
    let result = execute_graph(state, build_graph, config.process_limit, print_task_result);
    writeln!(stdout(), "{}", state.statistic)?;
    result
}

fn print_task_result(result: &BuildResult) -> crate::Result<()> {
    result.result.print_output()?;
    Ok(())
}
