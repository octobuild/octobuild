#![feature(core)]
#![feature(exit_status)]
#![feature(io)]
#![feature(os)]
extern crate octobuild;
extern crate tempdir;

use octobuild::common::BuildTask;
use octobuild::cache::Cache;
use octobuild::wincmd;
use octobuild::xg;
use octobuild::graph::{Graph, NodeIndex, Node, EdgeIndex, Edge};
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::*;

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
	println!("XGConsole:");
	let args = Vec::from_iter(env::args());
	for arg in args.iter() {
		println!("  {}", arg);
	}
	match execute(&args.as_slice()[1..]) {
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

		create_threads(rx_task, tx_result, std::os::num_cpus(), |worker_id:usize| {
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
		match try! (execute_graph(&validated_graph, tx_task, rx_result)) {
			Some(v) if v == 0 => {}
			v => {return Ok(v)}
		}
	}
	cache.cleanup(16 * 1024 * 1024 * 1024);
	Ok(Some(0))
}

fn create_threads<R: 'static + Send, T: 'static + Send, Worker:'static + Fn(T) -> R + Send, Factory:Fn(usize) -> Worker>(rx_task: Receiver<T>, tx_result: Sender<R>, num_cpus: usize, factory: Factory) ->  Arc<Mutex<Receiver<T>>> {
	let mutex_rx_task = Arc::new(Mutex::new(rx_task));
	for cpu_id in range(0, num_cpus) {
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
	graph.each_node(|index: NodeIndex, _:&Node<BuildTask>|->bool {
		completed.push(false);
		queue.push(index);
		true
	});
	let mut count:usize = 0;
	let mut i:usize = 0;
	while i < queue.len() {
		let index = queue[i];
		if (!completed[index.node_id()]) && (is_ready(&graph, &completed, &index)) {
			completed[index.node_id()] = true;
			graph.each_incoming_edge(index, |_:EdgeIndex, edge:&Edge<()>| -> bool {
				queue.push(edge.source());
				true
			});
			count += 1;
			if count ==completed.len() {
				return Ok(graph);
			}
		}
		i = i + 1;
	}
	Err(Error::new(ErrorKind::InvalidInput, "Found cycles in build dependencies", None))
}

fn execute_task(cache: &Cache, temp_dir: &Path, worker: usize, message: TaskMessage) -> ResultMessage {
	let args = wincmd::expand_args(&message.task.args, &|name:&str|->Option<String>{env::var(name).ok()});
	let output = execute_compiler(cache, temp_dir, message.task.exec.as_slice(), &Path::new(&message.task.working_dir), args.as_slice());
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
			.args(args.as_slice())
			.output()
			.map(|o| OutputInfo::new(o))
	}
}

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: Receiver<ResultMessage>) -> Result<Option<i32>, Error> {
	// Run all tasks.
	let result = execute_until_failed(graph, tx_task, &rx_result);
	// Wait for in progress task completion.
	for _ in rx_result.iter() {
	}
	result
}

fn execute_until_failed(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: &Receiver<ResultMessage>) -> Result<Option<i32>, Error> {
	let mut completed:Vec<bool> = Vec::new();
	if !graph.each_node(|index: NodeIndex, node:&Node<BuildTask>|->bool {
		let mut has_edges = false;
		graph.each_outgoing_edge(index, |_:EdgeIndex, _:&Edge<()>| -> bool {
			has_edges = true;
			false
		});
		completed.push(false);
		if !has_edges {
			match tx_task.send(TaskMessage{
				index: index,
				task: node.data.clone(),
			}) {
				Ok(_) => true,
				Err(_) => false,
			}
		} else {
			true
		}
	}) {
		return Err(Error::new(ErrorKind::BrokenPipe, "Can't schedule root tasks", None));
	}

	let mut count:usize = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.node_id()]);
		count += 1;
		println!("#{} {}/{}: {}", message.worker, count, completed.len(), message.task.title);
		let result = try! (message.result);
		try! (io::stdout().write_all(result.stdout.as_slice()));
		try! (io::stderr().write_all(result.stderr.as_slice()));
		if !result.success() {
			return Ok(result.status);
		}
		completed[message.index.node_id()] = true;
		if !graph.each_incoming_edge(message.index, |_:EdgeIndex, edge:&Edge<()>| -> bool {
			let source = edge.source();
			if !completed[source.node_id()] {
				if is_ready(graph, &completed, &source) {
					match tx_task.send(TaskMessage{
						index: source,
						task: graph.node(source).data.clone(),
					}) {
						Ok(_) => {},
						Err(_) => {return false;}
					}
				}
			}
			true
		}) {
			return Err(Error::new(ErrorKind::BrokenPipe, "Can't schedule child task", None));
		};
		if count == completed.len() {
			return Ok(Some(0));
		}
	}
	panic! ("Unexpected end of result pipe");
}

fn is_ready(graph: &Graph<BuildTask, ()>, completed: &Vec<bool>, source: &NodeIndex) -> bool {
	let mut ready = true;
		graph.each_outgoing_edge(*source, |_:EdgeIndex, deps:&Edge<()>| -> bool {
		if !completed[deps.target().node_id()]{
			ready = false;
			false
		} else {
			true
		}
	});
	ready
}
