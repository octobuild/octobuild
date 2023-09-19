#![allow(non_snake_case)]

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process;
use std::sync::Arc;

use petgraph::graph::NodeIndex;
use petgraph::{EdgeDirection, Graph};

use octobuild::cluster::client::RemoteCompiler;
use octobuild::compiler::{CommandArgs, Compiler, SharedState};
use octobuild::config::Config;
use octobuild::simple::supported_compilers;
use octobuild::version;
use octobuild::worker::execute_graph;
use octobuild::worker::validate_graph;
use octobuild::worker::{BuildAction, BuildGraph, BuildResult, BuildTask};
use octobuild::xg;
use octobuild::xg::parser::{XgGraph, XgNode};

pub fn main() -> octobuild::Result<()> {
    println!("xgConsole ({}):", version::full());
    let args: Vec<String> = env::args().collect();
    for arg in &args {
        println!("  {arg}");
    }
    if args.len() == 1 {
        Config::help(&args[0]);
        return Ok(());
    }

    process::exit(match execute(&args[1..]) {
        Ok(_) => 0,
        Err(e) => {
            println!("ERROR: {e}");
            1
        }
    })
}

fn execute(args: &[String]) -> octobuild::Result<()> {
    let config = Config::load()?;
    let state = SharedState::new(&config)?;
    let compiler = RemoteCompiler::new(&config.coordinator, supported_compilers());

    match args.get(0) {
        None => Err(octobuild::Error::NoTaskFiles),
        Some(arg) => {
            if arg.eq_ignore_ascii_case("/reset") {
                println!("Cleaning cache directory: {}...", config.cache.display());
                _ = std::fs::remove_dir_all(&config.cache);
                println!("Done!");
                Ok(())
            } else {
                let mut graph = Graph::new();
                let file = File::open(Path::new(&args[0]))?;
                xg::parser::parse(&mut graph, BufReader::new(file))?;
                let build_graph = validate_graph(graph)
                    .and_then(|graph| prepare_graph(&compiler, &graph, &config))?;

                let result =
                    execute_graph(&state, build_graph, config.process_limit, print_task_result);
                drop(state.cache.cleanup());
                println!("{}", state.statistic);
                result
            }
        }
    }
}

fn env_resolver(name: &str) -> Option<String> {
    env::var(name).ok()
}

fn prepare_graph<C: Compiler>(
    compiler: &C,
    graph: &XgGraph,
    config: &Config,
) -> octobuild::Result<BuildGraph> {
    let mut remap: Vec<NodeIndex> = Vec::with_capacity(graph.node_count());
    let mut depends: Vec<NodeIndex> = Vec::with_capacity(graph.node_count());

    let mut result: BuildGraph = Graph::new();
    for raw_node in graph.raw_nodes() {
        let node: &XgNode = &raw_node.weight;
        let raw_args: String = expand_arg(&node.raw_args, &env_resolver);
        let command = node.command.clone();

        let actions = BuildAction::create_tasks(
            compiler,
            command.clone(),
            CommandArgs::Raw(raw_args),
            &node.title,
            config.run_second_cpp,
        );
        let node_index = NodeIndex::new(remap.len());
        if actions.len() == 1 {
            depends.push(node_index);
            remap.push(result.add_node(Arc::new(BuildTask {
                title: node.title.clone(),
                action: actions.into_iter().next().unwrap(),
            })));
        } else {
            // Add group node for tracking end of all task actions
            let group_node = result.add_node(Arc::new(BuildTask {
                title: node.title.clone(),
                action: BuildAction::Empty,
            }));
            depends.push(NodeIndex::end());
            // Add task actions
            let mut index = 1;
            let total = actions.len();
            for action in actions {
                let action_node = result.add_node(Arc::new(BuildTask {
                    title: format!("{} ({index}/{total})", node.title),
                    action,
                }));
                depends.push(node_index);
                result.add_edge(group_node, action_node, ());
                index += 1;
            }
            remap.push(group_node);
        }
    }

    assert_eq!(remap.len(), graph.node_count());
    assert_eq!(depends.len(), result.node_count());
    for i in 0..depends.len() {
        let node_a = NodeIndex::new(i);
        for neighbor in graph.neighbors_directed(*depends.get(i).unwrap(), EdgeDirection::Outgoing)
        {
            let node_b = remap.get(neighbor.index()).unwrap();
            result.add_edge(node_a, *node_b, ());
        }
    }
    validate_graph(result)
}

fn print_task_result(result: &BuildResult) -> octobuild::Result<()> {
    println!(
        "#{} {}/{}: {} @ {}s",
        result.worker,
        result.completed,
        result.total,
        result.task.title,
        result.result.duration.as_secs(),
    );
    result.result.print_output()?;
    Ok(())
}

fn expand_arg<F: Fn(&str) -> Option<String>>(arg: &str, resolver: &F) -> String {
    let mut result = String::new();
    let mut suffix = arg;
    loop {
        match suffix.find("$(") {
            Some(begin) => match suffix[begin..].find(')') {
                Some(end) => {
                    let name = &suffix[begin + 2..begin + end];
                    match resolver(name) {
                        Some(ref value) => {
                            result += &suffix[..begin];
                            result += value;
                        }
                        None => {
                            result += &suffix[..=begin + end];
                        }
                    }
                    suffix = &suffix[begin + end + 1..];
                }
                None => {
                    result += suffix;
                    break;
                }
            },
            None => {
                result += suffix;
                break;
            }
        }
    }
    result
}

#[test]
fn test_parse_vars() {
    assert_eq!(
        expand_arg(
            "A$(test)$(inner)$(none)B",
            &|name: &str| -> Option<String> {
                match name {
                    "test" => Some("foo".to_string()),
                    "inner" => Some("$(bar)".to_string()),
                    "none" => None,
                    _ => {
                        unreachable!("Unexpected value: {}", name);
                    }
                }
            },
        ),
        "Afoo$(bar)$(none)B"
    );
}
