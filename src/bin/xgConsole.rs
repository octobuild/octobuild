extern crate octobuild;
extern crate petgraph;
extern crate tempdir;
extern crate regex;
#[macro_use]
extern crate lazy_static;

use octobuild::common::BuildTask;
use octobuild::config::Config;
use octobuild::cache::Cache;
use octobuild::xg;
use octobuild::version;
use octobuild::vs::compiler::VsCompiler;
use octobuild::io::statistic::Statistic;
use octobuild::clang::compiler::ClangCompiler;
use octobuild::compiler::*;

use petgraph::{Graph, EdgeDirection};
use petgraph::graph::NodeIndex;
use tempdir::TempDir;
use regex::Regex;

use std::fs::File;
use std::env;
use std::io::{BufReader, Error, ErrorKind, Write};
use std::io;
use std::iter::FromIterator;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::process;
use std::thread;

#[derive(Debug)]
struct TaskMessage {
	index: NodeIndex,
	task: BuildTask
}

#[derive(Debug)]
struct ResultMessage {
	index: NodeIndex,
	task: BuildTask,
	worker: usize,
	result: Result<OutputInfo, Error>
}

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
		Ok(result) => {
			match result {
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

fn is_flag(arg: &str) -> bool {
  lazy_static! {
    static ref RE: Regex = Regex::new(r"^/\w+([=].*)?$").unwrap();
  }
  RE.is_match(arg)
}

fn execute(args: &[String]) -> Result<Option<i32>, Error> {
	let statistic = Arc::new(RwLock::new(Statistic::new()));
	let config = try! (Config::new());
	let cache = Cache::new(&config);
	let temp_dir = try! (TempDir::new("octobuild"));
	let files = Vec::from_iter(args.iter().filter(|a| !is_flag(a)));
	if files.len() == 0 {
		return Err(Error::new(ErrorKind::InvalidInput, "Build task files not found"));
	}

	let mut graph = Graph::new();
	for arg in files.iter() {
		let file = try! (File::open(&Path::new(arg)));
		try! (xg::parser::parse(&mut graph, BufReader::new(file)));
	}
	let validated_graph = try! (validate_graph(graph));

	let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
	let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();
	let mutex_rx_task = create_threads(rx_task, tx_result, config.process_limit, |worker_id:usize| {
		let temp_path = temp_dir.path().to_path_buf();
		let temp_cache = cache.clone();
		let temp_statistic = statistic.clone();
		move |task:TaskMessage| -> ResultMessage {
			execute_task(&temp_cache, &temp_path, worker_id, task, &temp_statistic)
		}
	});

	match try! (execute_graph(&validated_graph, tx_task, mutex_rx_task, rx_result)) {
		Some(v) if v == 0 => {}
		v => {return Ok(v)}
	}
	cache.cleanup();
	println!("{}", statistic.read().unwrap().to_string());
	Ok(Some(0))
}

fn create_threads<R: 'static + Send, T: 'static + Send, Worker:'static + Fn(T) -> R + Send, Factory:Fn(usize) -> Worker>(rx_task: Receiver<T>, tx_result: Sender<R>, num_cpus: usize, factory: Factory) ->  Arc<Mutex<Receiver<T>>> {
	let mutex_rx_task = Arc::new(Mutex::new(rx_task));
	for cpu_id in 0..num_cpus {
		let local_rx_task = mutex_rx_task.clone();
		let local_tx_result = tx_result.clone();
		let worker = factory(cpu_id);
		thread::spawn(move || {
			loop {
				let task: T;
				match local_rx_task.lock().unwrap().recv() {
					Ok(v) => {task = v;}
					Err(_) => {break;}
				}
				match local_tx_result.send(worker(task)) {
					Ok(_) => {}
					Err(_) => {break;}
				}
			}
		});
	}
	mutex_rx_task
}

fn validate_graph(graph: Graph<BuildTask, ()>) -> Result<Graph<BuildTask, ()>, Error> {
	let mut completed:Vec<bool> = Vec::new();
	let mut queue:Vec<NodeIndex> = Vec::new();
	for index in 0 .. graph.node_count() {
		completed.push(false);
		queue.push(NodeIndex::new(index));
	}
	let mut count:usize = 0;
	let mut i:usize = 0;
	while i < queue.len() {
		let index = queue[i];
		if (!completed[index.index()]) && (is_ready(&graph, &completed, &index)) {
			completed[index.index()] = true;
			for neighbor in graph.neighbors_directed(index, EdgeDirection::Incoming) {
				queue.push(neighbor);
			}
			count += 1;
			if count == completed.len() {
				return Ok(graph);
			}
		}
		i = i + 1;
	}
	Err(Error::new(ErrorKind::InvalidInput, "Found cycles in build dependencies"))
}

fn execute_task(cache: &Cache, temp_dir: &Path, worker: usize, message: TaskMessage, statistic: &RwLock<Statistic>) -> ResultMessage {
	let args = expand_args(&message.task.args, &|name:&str|->Option<String>{env::var(name).ok()});
	let output = execute_compiler(cache, temp_dir, &message.task, &args, statistic);
	ResultMessage {
		index: message.index,
		task: message.task,
		worker: worker,
		result: output,
	}
}

fn execute_compiler(cache: &Cache, temp_dir: &Path, task: &BuildTask, args: &[String], statistic: &RwLock<Statistic>) -> Result<OutputInfo, Error> {
	let command = CommandInfo {
		program: Path::new(&task.exec).to_path_buf(),
		current_dir: Some(Path::new(&task.working_dir).to_path_buf()),
		env: task.env.clone(),
	};
	let exec = Path::new(&task.exec);
	if exec.ends_with("cl.exe") {
		let compiler = VsCompiler::new(cache, temp_dir);
		compiler.compile(command, args, statistic)
	} else if exec.file_name().map_or(None, |name| name.to_str()).map_or(false, |name| name.starts_with("clang")) {
		let compiler = ClangCompiler::new(cache);
		compiler.compile(command, args, statistic)
	} else {
		command.to_command()
			.args(&args)
			.output()
			.map(|o| OutputInfo::new(o))
	}
}

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, mutex_rx_task: Arc<Mutex<Receiver<TaskMessage>>>, rx_result: Receiver<ResultMessage>) -> Result<Option<i32>, Error> {
	// Run all tasks.
	let mut count: usize = 0;
	let result = execute_until_failed(graph, tx_task, &rx_result, &mut count);
	// Cleanup task queue.
	for _ in mutex_rx_task.lock().unwrap().iter() {
	}
	// Wait for in progress task completion.
	for message in rx_result.iter() {
		try! (print_task_result(&message, &mut count, graph.node_count()));
	}
	result
}

fn execute_until_failed(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: &Receiver<ResultMessage>, count: &mut usize) -> Result<Option<i32>, Error> {
	let mut completed:Vec<bool> = Vec::new();
	for _ in 0 .. graph.node_count() {
		completed.push(false);
	}
	for index in graph.externals(EdgeDirection::Outgoing) {
		try! (tx_task.send(TaskMessage{
			index: index,
			task: graph.node_weight(index).unwrap().clone(),
		}).map_err(|e| Error::new(ErrorKind::Other, e)));
	}

	for message in rx_result.iter() {
		assert!(!completed[message.index.index()]);
		try! (print_task_result(&message, count, graph.node_count()));
		let result = try! (message.result);
		if !result.success() {
			return Ok(result.status);
		}
		completed[message.index.index()] = true;

		for source in graph.neighbors_directed(message.index, EdgeDirection::Incoming) {
			if is_ready(graph, &completed, &source) {
				try! (tx_task.send(TaskMessage{
					index: source,
					task: graph.node_weight(source).unwrap().clone(),
				}).map_err(|e| Error::new(ErrorKind::Other, e)));
			}
		}

		if *count == completed.len() {
			return Ok(Some(0));
		}
	}
	panic! ("Unexpected end of result pipe");
}

fn print_task_result(message: &ResultMessage, completed: &mut usize, total: usize) -> Result<(), Error> {
	*completed += 1;
	println!("#{} {}/{}: {}", message.worker, completed, total, message.task.title);
	match message.result {
		Ok(ref output) => {
			try! (io::stdout().write_all(&output.stdout));
			try! (io::stderr().write_all(&output.stderr));
		}
		Err(_) => {
		}
	}
	Ok(())
}

fn is_ready(graph: &Graph<BuildTask, ()>, completed: &Vec<bool>, source: &NodeIndex) -> bool {
	for neighbor in graph.neighbors_directed(*source, EdgeDirection::Outgoing) {
		if !completed[neighbor.index()]{
			return false
		}
	}
	true
}

fn expand_arg<F: Fn(&str) -> Option<String>>(arg: &str, resolver: &F) -> String {
	let mut result = String::new();
	let mut suffix = arg;
	loop {
		match suffix.find("$(") {
			Some(begin) => {
				match suffix[begin..].find(")") {
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
						result = result+suffix;
						break;
					}
				}
			}
			None => {
				result = result+ suffix;
				break;
			}
		}
	}
	result
}

fn expand_args<F: Fn(&str) -> Option<String>>(args: &Vec<String>, resolver: &F) -> Vec<String> {
	let mut result:Vec<String> = Vec::new();
	for arg in args.iter() {
		result.push(expand_arg(&arg, resolver));
	}
	result
}

#[test]
fn test_parse_vars() {
	assert_eq!(expand_arg("A$(test)$(inner)$(none)B", &|name:&str|->Option<String> {
		match name {
			"test" => {
				Some("foo".to_string())
			}
			"inner" => {
				Some("$(bar)".to_string())
			}
			"none" => {
				None
			}
			_ => {
				assert!(false, format!("Unexpected value: {}", name));
				None
			}
		}
	}), "Afoo$(bar)$(none)B");
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
