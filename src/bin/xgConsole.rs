use std::env;
use std::fs::File;
use std::io;
use std::io::{BufReader, Error, ErrorKind, Write};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;

use petgraph::graph::NodeIndex;
use petgraph::{EdgeDirection, Graph};
use regex::Regex;

use lazy_static::lazy_static;
use octobuild::cluster::client::RemoteCompiler;
use octobuild::compiler::*;
use octobuild::config::Config;
use octobuild::simple::create_temp_dir;
use octobuild::simple::supported_compilers;
use octobuild::version;
use octobuild::worker::execute_graph;
use octobuild::worker::validate_graph;
use octobuild::worker::{BuildAction, BuildGraph, BuildResult, BuildTask};
use octobuild::xg;
use octobuild::xg::parser::{XgGraph, XgNode};

fn main() {
    println!("xgConsole ({}):", version::full_version());
    let args = Vec::from_iter(env::args());
    for arg in args.iter() {
        println!("  {}", arg);
    }
    if args.len() == 1 {
        println!("");
        Config::help();
        return;
    }

    process::exit(match execute(&args[1..]) {
        Ok(result) => match result {
            Some(r) => r,
            None => 501,
        },
        Err(e) => {
            println!("FATAL ERROR: {}", e);
            500
        }
    })
}

fn is_flag(arg: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^/\w+([=].*)?$").unwrap();
    }
    RE.is_match(arg)
}

#[cfg(unix)]
fn expand_files(mut files: Vec<PathBuf>, arg: &str) -> Vec<PathBuf> {
    files.push(Path::new(arg).to_path_buf());
    files
}

#[cfg(windows)]
fn expand_files(mut files: Vec<PathBuf>, arg: &str) -> Vec<PathBuf> {
    use std::fs;

    fn mask_to_regex(mask: &str) -> Regex {
        let mut result = String::new();
        let mut begin = 0;
        result.push_str("^");
        for (index, separator) in mask.match_indices(|c| c == '?' || c == '*') {
            result.push_str(&regex::escape(&mask[begin..index]));
            result.push_str(match separator {
                "?" => ".",
                "*" => ".*",
                unknown => panic!("Unexpected separator: {}", unknown),
            });
            begin = index + separator.len()
        }
        result.push_str(&regex::escape(&mask[begin..]));
        result.push_str("$");
        return Regex::new(&result).unwrap();
    }

    fn find_files(dir: &Path, mask: &str) -> Result<Vec<PathBuf>, Error> {
        let mut result = Vec::new();
        let expr = mask_to_regex(&mask.to_lowercase());
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry
                .file_name()
                .to_str()
                .map_or(false, |s| expr.is_match(&s.to_lowercase()))
            {
                result.push(entry.path());
            }
        }
        Ok(result)
    }

    let path = Path::new(arg).to_path_buf();
    let mask = path
        .file_name()
        .map_or(None, |name| name.to_str())
        .map_or(None, |s| Some(s.to_string()));
    match mask {
        Some(ref mask) if mask.contains(|c| c == '?' || c == '*') => {
            match find_files(path.parent().unwrap_or(Path::new(".")), mask) {
                Ok(ref mut found) if found.len() > 0 => {
                    files.append(found);
                }
                _ => {
                    files.push(path);
                }
            }
        }
        _ => {
            files.push(path);
        }
    }
    files
}

fn execute(args: &[String]) -> Result<Option<i32>, Error> {
    let config = Config::new()?;
    let state = SharedState::new(&config)?;
    let compiler = RemoteCompiler::new(&config.coordinator, supported_compilers(&create_temp_dir()?));
    let files = args
        .iter()
        .filter(|a| !is_flag(a))
        .fold(Vec::new(), |state, a| expand_files(state, &a));
    if files.len() == 0 {
        return Err(Error::new(ErrorKind::InvalidInput, "Build task files not found"));
    }

    let mut graph = Graph::new();
    for arg in files.iter() {
        let file = File::open(&Path::new(arg))?;
        xg::parser::parse(&mut graph, BufReader::new(file))?;
    }
    let build_graph = validate_graph(graph).and_then(|graph| prepare_graph(&compiler, graph))?;

    let result = execute_graph(&state, build_graph, config.process_limit, print_task_result);
    let _ = state.cache.cleanup();
    println!("{}", state.statistic.to_string());
    result
}

fn env_resolver(name: &str) -> Option<String> {
    env::var(name).ok()
}

fn prepare_graph<C: Compiler>(compiler: &C, graph: XgGraph) -> Result<BuildGraph, Error> {
    let mut remap: Vec<NodeIndex> = Vec::with_capacity(graph.node_count());
    let mut depends: Vec<NodeIndex> = Vec::with_capacity(graph.node_count());

    let mut result: BuildGraph = Graph::new();
    for raw_node in graph.raw_nodes().iter() {
        let node: &XgNode = &raw_node.weight;
        let args: Vec<String> = node
            .args
            .iter()
            .map(|ref arg| expand_arg(&arg, &env_resolver))
            .collect();
        let command = node.command.clone();

        let actions = BuildAction::create_tasks(compiler, command.clone(), &args, &node.title);
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
            for action in actions.into_iter() {
                let action_node = result.add_node(Arc::new(BuildTask {
                    title: format!("{} ({}/{})", node.title, index, total),
                    action,
                }));
                depends.push(node_index);
                result.add_edge(group_node, action_node, ());
                index += 1;
            }
            remap.push(group_node);
        }
    }

    assert!(remap.len() == graph.node_count());
    assert!(depends.len() == result.node_count());
    for i in 0..depends.len() {
        let node_a = NodeIndex::new(i);
        for neighbor in graph.neighbors_directed(depends.get(i).unwrap().clone(), EdgeDirection::Outgoing) {
            let node_b = remap.get(neighbor.index()).unwrap();
            result.add_edge(node_a, node_b.clone(), ());
        }
    }
    validate_graph(result)
}

fn print_task_result(result: BuildResult) -> Result<(), Error> {
    println!(
        "#{} {}/{}: {}",
        result.worker, result.completed, result.total, result.task.title
    );
    match result.result {
        &Ok(ref output) => {
            io::stdout().write_all(&output.stdout)?;
            io::stderr().write_all(&output.stderr)?;
        }
        &Err(_) => {}
    }
    Ok(())
}

fn expand_arg<F: Fn(&str) -> Option<String>>(arg: &str, resolver: &F) -> String {
    let mut result = String::new();
    let mut suffix = arg;
    loop {
        match suffix.find("$(") {
            Some(begin) => match suffix[begin..].find(")") {
                Some(end) => {
                    let name = &suffix[begin + 2..begin + end];
                    match resolver(name) {
                        Some(ref value) => {
                            result = result + &suffix[..begin] + &value;
                        }
                        None => {
                            result = result + &suffix[..begin + end + 1];
                        }
                    }
                    suffix = &suffix[begin + end + 1..];
                }
                None => {
                    result = result + suffix;
                    break;
                }
            },
            None => {
                result = result + suffix;
                break;
            }
        }
    }
    result
}

#[test]
fn test_parse_vars() {
    assert_eq!(
        expand_arg("A$(test)$(inner)$(none)B", &|name: &str| -> Option<String> {
            match name {
                "test" => Some("foo".to_string()),
                "inner" => Some("$(bar)".to_string()),
                "none" => None,
                _ => {
                    assert!(false, format!("Unexpected value: {}", name));
                    None
                }
            }
        }),
        "Afoo$(bar)$(none)B"
    );
}

#[test]
fn test_is_flag() {
    assert_eq!(is_flag("/Wait"), true);
    assert_eq!(is_flag("/out=/foo/bar"), true);
    assert_eq!(is_flag("/out/foo/bar"), false);
    assert_eq!(is_flag("foo/bar"), false);
    assert_eq!(is_flag("/Wait.xml"), false);
    assert_eq!(is_flag("/Wait/foo=bar"), false);
    assert_eq!(is_flag("/WaitFoo=bar"), true);
    assert_eq!(is_flag("/Wait.Foo=bar"), false);
    assert_eq!(is_flag("/Wait=/foo/bar"), true);
}
