#![feature(core)]
#![feature(old_io)]
#![feature(old_path)]
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

use std::old_io::{stdout, stderr, Command, File, BufferedReader, IoError, IoErrorKind, TempDir};
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
	result: Result<BuildResult, IoError>
}

#[derive(Debug)]
struct BuildResult {
	exit_code: ProcessExit,
	stdout: Vec<u8>,
	stderr: Vec<u8>,
}

fn main() {
	println!("XGConsole:");
	let args = os::args();
	for arg in args.iter() {
		println!("  {}", arg);
	}
	match execute(&args.as_slice()[1..]) {
		Ok(result) => {
			std::os::set_exit_status(match result {
				ProcessExit::ExitStatus(r) => r,
				ProcessExit::ExitSignal(r) => r
			});
		}
		Err(e) => {
			println!("FATAL ERROR: {:?}", e);
			std::os::set_exit_status(500);
		}
	}
}

fn execute(args: &[String]) -> Result<ProcessExit, IoError> {
	let cache = Cache::new();
	let temp_dir = try! (TempDir::new("octobuild"));
	for arg in args.iter() {
		if arg.starts_with("/") {continue}

		let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
		let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();

		create_threads(rx_task, tx_result, std::os::num_cpus(), |worker_id:usize| {
			let temp_path = temp_dir.path().clone();
			let temp_cache = cache.clone();
			move |task:TaskMessage| -> ResultMessage {
				execute_task(&temp_cache, &temp_path, worker_id, task)
			}
		});	

		let path = Path::new(arg);
		let file = try! (File::open(&path));
		let graph = try! (xg::parser::parse(BufferedReader::new(file)));
		let validated_graph = try! (validate_graph(graph));
		let result = try! (execute_graph(&validated_graph, tx_task, rx_result));
		if !result.success() {
			return Ok(result);
		}
	}
	Ok(ProcessExit::ExitStatus(0))
}

fn create_threads<R: 'static + Send, T: 'static + Send, Worker:'static + Fn(T) -> R + Send, Factory:Fn(usize) -> Worker>(rx_task: Receiver<T>, tx_result: Sender<R>, num_cpus: usize, factory: Factory) ->  Arc<Mutex<Receiver<T>>> {
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

fn validate_graph(graph: Graph<BuildTask, ()>) -> Result<Graph<BuildTask, ()>, IoError> {
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
	Err(IoError {
		kind: IoErrorKind::InvalidInput,
		desc: "Found cycles in build dependencies",
		detail: None
	})
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
				result: Err(e)
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

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: Receiver<ResultMessage>) -> Result<ProcessExit, IoError> {
	// Run all tasks.
	let result = execute_until_failed(graph, tx_task, &rx_result);
	// Wait for in progress task completion.
	for _ in rx_result.iter() {
	}
	result
}

fn execute_until_failed(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: &Receiver<ResultMessage>) -> Result<ProcessExit, IoError> {
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
		return Err(IoError {
			kind: IoErrorKind::BrokenPipe,
			desc: "Can't schedule root tasks",
			detail: None
		});
	}

	let mut count:usize = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.node_id()]);
		count += 1;
		println!("#{} {}/{}: {}", message.worker, count, completed.len(), message.task.title);
		let result = try! (message.result);
		try! (stdout().write_all(result.stdout.as_slice()));
		try! (stderr().write_all(result.stderr.as_slice()));
		if !result.exit_code.success() {
			return Ok(result.exit_code);
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
			return Err(IoError {
				kind: IoErrorKind::BrokenPipe,
				desc: "Can't schedule child task",
				detail: None
			});
		};
		if count == completed.len() {
			return Ok(ProcessExit::ExitStatus(0));
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
