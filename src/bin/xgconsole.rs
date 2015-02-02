#![feature(core)]
#![feature(collections)]
#![feature(io)]
#![feature(path)]
#![feature(os)]
#![feature(std_misc)]
extern crate octobuild;

use octobuild::common::BuildTask;
use octobuild::cache::Cache;
use octobuild::wincmd;
use octobuild::xg;
use octobuild::graph::{Graph, NodeIndex, Node, EdgeIndex, Edge};
use octobuild::vs::compiler::VsCompiler;
use octobuild::compiler::Compiler;

use std::os;

use std::old_io::{stdout, stderr, Command, File, BufferedReader, IoError, TempDir};
use std::old_io::process::{ProcessExit, ProcessOutput};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::Thread;

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
	result: Result<BuildResult, String>
}

#[derive(Debug)]
struct BuildResult {
	exit_code: ProcessExit,
	stdout: Vec<u8>,
	stderr: Vec<u8>,
}

fn main() {
	let temp_dir = match TempDir::new("octobuild") {
		Ok(result) => result,
		Err(e) => {panic!(e);}
	};

	println!("XGConsole:");
	for arg in os::args().iter() {
		println!("  {}", arg);
	}

	let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
	let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();
	let cache = Cache::new();

	let mutex_rx_task = create_threads(rx_task, tx_result, std::os::num_cpus(), |worker_id:usize| {
		let temp_path = temp_dir.path().clone();
		let temp_cache = cache.clone();
		move |task:TaskMessage| -> ResultMessage {
			execute_task(&temp_cache, &temp_path, worker_id, task)
		}
	});

	let args = os::args();
	if args.len() <= 1 {
		panic! ("Task file is not defined");
	}
	let path = Path::new(&args[1]);
	match File::open(&path) {
		Ok(file) => {
			match xg::parser::parse(BufferedReader::new(file)) {
				Ok(graph) => {
					match validate_graph(graph) {
						Ok(graph) => {
							execute_graph(&graph, tx_task, mutex_rx_task, rx_result);
						}
						Err(msg) =>{panic! (msg);}
					}
				}
				Err(msg) =>{panic! (msg);}
			}
		}
		Err(msg) =>{panic! (msg);}
	}
}

fn create_threads<R: Send, T: Send, Worker:Fn(T) -> R + Send, Factory:Fn(usize) -> Worker>(rx_task: Receiver<T>, tx_result: Sender<R>, num_cpus: usize, factory: Factory) ->  Arc<Mutex<Receiver<T>>> {
	let mutex_rx_task = Arc::new(Mutex::new(rx_task));
	for cpu_id in range(0, num_cpus) {
		let local_rx_task = mutex_rx_task.clone();
		let local_tx_result = tx_result.clone();
		let worker = factory(cpu_id);
		Thread::spawn(move || {
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

fn validate_graph(graph: Graph<BuildTask, ()>) -> Result<Graph<BuildTask, ()>, String> {
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
	return Err("Found cycles in build dependencies.".to_string());
}

fn execute_task(cache: &Cache, temp_dir: &Path, worker: usize, message: TaskMessage) -> ResultMessage {
	let args = wincmd::expand_args(&message.task.args, &|name:&str|->Option<String>{os::getenv(name)});
	match execute_compiler(cache, temp_dir, message.task.exec.as_slice(), &Path::new(&message.task.working_dir), args.as_slice()) {
		Ok(output) => {
			ResultMessage {
				index: message.index,
				task: message.task,
				worker: worker,
				result: Ok(BuildResult {
					exit_code: output.status,
					stdout: output.output,
					stderr: output.error
				})
			}
		}
		Err(e) => {
			ResultMessage {
				index: message.index,
				task: message.task,
				worker: worker,
				result: Err(format!("Failed to start process: {}", e))
			}
		}
	}
}

fn execute_compiler(cache: &Cache, temp_dir: &Path, program: &str, cwd: &Path, args: &[String]) -> Result<ProcessOutput, IoError> {
	let mut command = Command::new(program);
	command.cwd(cwd);
	if Path::new(program).ends_with_path(&Path::new("cl.exe")) {
		let compiler = VsCompiler::new(cache, temp_dir);
		compiler.compile(&command, args)
	} else {
		command
			.args(args.as_slice())
			.output()
	}
}

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_task: Arc<Mutex<Receiver<TaskMessage>>>, rx_result: Receiver<ResultMessage>) {
	let mut completed:Vec<bool> = Vec::new();
		graph. each_node(|index: NodeIndex, node:&Node<BuildTask>|->bool {
			let mut has_edges = false;
			graph.each_outgoing_edge(index, |_:EdgeIndex, _:&Edge<()>| -> bool {
			has_edges = true;
			false
		});
		if !has_edges {
			tx_task.send(TaskMessage{
				index: index,
				task: node.data.clone(),
			});
		}
		completed.push(false);
		true
	});
	let mut count:usize = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.node_id()]);
		count += 1;
		println!("#{} {}/{}: {}", message.worker, count, completed.len(), message.task.title);
		match message.result {
			Ok (result) => {
				stdout().write_all(result.stdout.as_slice());
				stderr().write_all(result.stderr.as_slice());
				if !result.exit_code.success() {
					break;
				}
				completed[message.index.node_id()] = true;
					graph.each_incoming_edge(message.index, |_:EdgeIndex, edge:&Edge<()>| -> bool {
					let source = edge.source();
					if !completed[source.node_id()] {
						if is_ready(graph, &completed, &source) {
							tx_task.send(TaskMessage{
								index: source,
								task: graph.node(source).data.clone(),
							});
						}
					}
					true
				});
			}
			Err (e) => {
				println!("{}", e);
				break;
			}
		}
		if count == completed.len() {
			break;
		}
	}
	// No more tasks.
	free(tx_task);
	// Cleanup task list.
	match rx_task.lock() {
		queue => {
			for _ in queue.iter() {
			}
		}
	}
	// Wait for in progress task completion.
	for _ in rx_result.iter() {
	}
}

fn free<T>(_:T) {
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
