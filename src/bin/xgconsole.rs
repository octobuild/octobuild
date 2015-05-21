#![feature(exit_status)]
extern crate octobuild;
extern crate petgraph;
extern crate tempdir;

use octobuild::common::BuildTask;
use octobuild::cache::Cache;
use octobuild::wincmd;
use octobuild::xg;
use octobuild::utils;
use octobuild::version;
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::*;


use petgraph::{Graph, EdgeDirection};
use petgraph::graph::NodeIndex;
use tempdir::TempDir;

use std::fs::File;
use std::env;
use std::io::{BufReader, Error, ErrorKind, Write};
use std::io;
use std::iter::FromIterator;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
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
	println!("XGConsole ({}):", version::full_version());
	let args = Vec::from_iter(env::args());
	for arg in args.iter() {
		println!("  {}", arg);
	}
	match execute(&args[1..]) {
		Ok(result) => {
			env::set_exit_status(match result {
				Some(r) => r,
				None => 500
			});
		}
		Err(e) => {
			println!("FATAL ERROR: {:?}", e);
			env::set_exit_status(500);
		}
	}
}

fn execute(args: &[String]) -> Result<Option<i32>, Error> {
	let cache = Cache::new();
	let temp_dir = try! (TempDir::new("octobuild"));
	for arg in args.iter() {
		if arg.starts_with("/") {continue}

		let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
		let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();

		let mutex_rx_task = create_threads(rx_task, tx_result, utils::num_cpus(), |worker_id:usize| {
			let temp_path = temp_dir.path().to_path_buf();
			let temp_cache = cache.clone();
			move |task:TaskMessage| -> ResultMessage {
				execute_task(&temp_cache, &temp_path, worker_id, task)
			}
		});	

		let path = Path::new(arg);
		let file = try! (File::open(&path));
		let graph = try! (xg::parser::parse(BufReader::new(file)));
		let validated_graph = try! (validate_graph(graph));
		match try! (execute_graph(&validated_graph, tx_task, mutex_rx_task, rx_result)) {
			Some(v) if v == 0 => {}
			v => {return Ok(v)}
		}
	}
	cache.cleanup(16 * 1024 * 1024 * 1024);
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
			if count ==completed.len() {
				return Ok(graph);
			}
		}
		i = i + 1;
	}
	Err(Error::new(ErrorKind::InvalidInput, "Found cycles in build dependencies"))
}

fn execute_task(cache: &Cache, temp_dir: &Path, worker: usize, message: TaskMessage) -> ResultMessage {
	let args = wincmd::expand_args(&message.task.args, &|name:&str|->Option<String>{env::var(name).ok()});
	let output = execute_compiler(cache, temp_dir, &message.task.exec, &Path::new(&message.task.working_dir), &args);
	ResultMessage {
		index: message.index,
		task: message.task,
		worker: worker,
		result: output,
	}
}

fn execute_compiler(cache: &Cache, temp_dir: &Path, program: &str, cwd: &Path, args: &[String]) -> Result<OutputInfo, Error> {
	let command = CommandInfo {
		program: Path::new(program).to_path_buf(),
		current_dir: Some(cwd.to_path_buf()),
	};
	if Path::new(program).ends_with("cl.exe") {
		let compiler = VsCompiler::new(cache, temp_dir);
		compiler.compile(command, args)
	} else {
		command.to_command()
			.args(&args)
			.output()
			.map(|o| OutputInfo::new(o))
	}
}

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, mutex_rx_task: Arc<Mutex<Receiver<TaskMessage>>>, rx_result: Receiver<ResultMessage>) -> Result<Option<i32>, Error> {
	// Run all tasks.
	let result = execute_until_failed(graph, tx_task, &rx_result);
	// Cleanup task queue.
	for _ in mutex_rx_task.lock().unwrap().iter() {
	}
	// Wait for in progress task completion.
	for _ in rx_result.iter() {
	}
	result
}

fn execute_until_failed(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: &Receiver<ResultMessage>) -> Result<Option<i32>, Error> {
	let mut completed:Vec<bool> = Vec::new();
	for _ in 0 .. graph.node_count() {
		completed.push(false);
	}
	for index in graph.without_edges(EdgeDirection::Incoming) {
		try! (tx_task.send(TaskMessage{
			index: index,
			task: graph.node_weight(index).unwrap().clone(),
		}).map_err(|e| Error::new(ErrorKind::Other, e)));
	}

	let mut count:usize = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.index()]);
		count += 1;
		println!("#{} {}/{}: {}", message.worker, count, completed.len(), message.task.title);
		let result = try! (message.result);
		try! (io::stdout().write_all(&result.stdout));
		try! (io::stderr().write_all(&result.stderr));
		if !result.success() {
			return Ok(result.status);
		}
		completed[message.index.index()] = true;

		for source in graph.neighbors_directed(message.index, EdgeDirection::Incoming) {
			if !completed[source.index()] {
				if is_ready(graph, &completed, &source) {
					try! (tx_task.send(TaskMessage{
						index: source,
						task: graph.node_weight(source).unwrap().clone(),
					}).map_err(|e| Error::new(ErrorKind::Other, e)));
				}
			}
		}

		if count == completed.len() {
			return Ok(Some(0));
		}
	}
	panic! ("Unexpected end of result pipe");
}

fn is_ready(graph: &Graph<BuildTask, ()>, completed: &Vec<bool>, source: &NodeIndex) -> bool {
	for neighbor in graph.neighbors_directed(*source, EdgeDirection::Outgoing) {
		if !completed[neighbor.index()]{
			return false
		}
	}
	true
}
