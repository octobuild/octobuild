#![allow(unstable)]
extern crate octobuild;

use octobuild::common::{BuildTask};
use octobuild::wincmd;
use octobuild::xg;
use octobuild::graph::{Graph, NodeIndex, Node, EdgeIndex, Edge};

use std::os;

use std::io::{Command, File, BufferedReader};
use std::io::process::ProcessExit;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::Thread;

#[derive(Show)]
struct TaskMessage {
index: NodeIndex,
task: BuildTask
}

#[derive(Show)]
struct ResultMessage {
index: NodeIndex,
result: Result<BuildResult, String>
}

#[derive(Show)]
struct BuildResult {
exit_code: ProcessExit,
}

fn main() {
	println!("XGConsole:");
	for arg in os::args().iter() {
		println!("  {}", arg);
	}

	let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
	let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();

	let mutex_rx_task = create_threads(rx_task, tx_result, std::os::num_cpus(), |worker_id:usize| {
		move |task:TaskMessage| -> ResultMessage {
			println!("{}: {:?}", worker_id, task.task.title);
			execute_task(task)
		}
	});

	let args = os::args();
	let mut path;
	if args.len() <= 1 {
				path = Path::new(&args[0]).dir_path();
				path.push("../tests/graph-parser.xml");
	} else {
			path =Path::new(&args[1]);
	}
	println!("Example path: {}", path.display());
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

	println!("done");
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
    						Ok(v) => {task = v;
    					}
    						Err(_) => {
    						break;
    					}
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
		graph. each_node(|index: NodeIndex, _:&Node<BuildTask>|->bool {
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

fn execute_task(message: TaskMessage) -> ResultMessage {
	println!("{}", message.task.title);

	let args = wincmd::expand_args(&message.task.args, &|name:&str|->Option<String>{os::getenv(name)});
	match Command::new(message.task.exec)
	.args(args.as_slice())
	.cwd(&Path::new(&message.task.working_dir))
	.output(){
			Ok(output) => {
			println!("stdout: {}", String::from_utf8_lossy(output.output.as_slice()));
			println!("stderr: {}", String::from_utf8_lossy(output.error.as_slice()));
			ResultMessage {
			index: message.index,
			result: Ok(BuildResult {
			exit_code: output.status
			})
			}}
			Err(e) => {
			ResultMessage {
			index: message.index,
			result: Err(format!("Failed to start process: {}", e))}
		}
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
			})  ;
		}
			completed.push(false);
		true
	});
	let mut count:usize = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.node_id()]);
		count += 1;
		println!("R {}/{}: {:?}", count, completed.len(), message);
		match message.result {
				Ok (result) => {
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
							})  ;
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
