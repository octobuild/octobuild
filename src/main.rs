extern crate xml;
extern crate rustc;

use std::os;

use std::io::{Command, File, BufferedReader};
use std::io::process::ProcessExit;
use std::fmt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::Thread;

use rustc::middle::graph::{Graph, NodeIndex, Node, EdgeIndex, Edge};

use xml::reader::EventReader;
use xml::reader::events::XmlEvent;

struct TaskMessage {
index: NodeIndex,
task: BuildTask
}

impl fmt::Show for TaskMessage {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "index={}, title={}", self .index, self .task.title)
}
}

struct ResultMessage {
index: NodeIndex,
result: Result<BuildResult, String>
}

impl fmt::Show for ResultMessage {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "index={}, result={}", self .index, self .result)
}
}

struct BuildResult {
exit_code: ProcessExit,
}

impl fmt::Show for BuildResult {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "exit_code={}", self .exit_code)
}
}

fn main() {
	println!("XGConsole:");
	for arg in parse_command_line(os::args()).iter() {
		println!("  {}", arg);
	}

	let (tx_result, rx_result): (Sender<ResultMessage>, Receiver<ResultMessage>) = channel();
	let (tx_task, rx_task): (Sender<TaskMessage>, Receiver<TaskMessage>) = channel();

	let mutex_rx_task = Arc::new(Mutex::new(rx_task));
	for cpu_id in range(0, std::os::num_cpus()) {
		let local_rx_task = mutex_rx_task.clone();
		let local_tx_result = tx_result .clone();
				Thread::spawn(move || {
				loop {
					let message: TaskMessage;
					match local_rx_task.lock().recv_opt() {
							Ok(v) => {message = v;
						}
							Err(_) => {
							break;
						}
						}
					println!("{}: {}", cpu_id, message);
					local_tx_result.send(execute_task(message));
				}
			}).detach();
	}
	free(tx_result);

	let args = os::args();
	let mut path;
	if args.len() <= 1 {
				path = Path::new(&args[0]).dir_path();
				path.push("../tests/graph-parser.xml");
	} else {
			path =Path::new(&args[1]);
	}
	println!("Example path: {}", path.display());
	match xg_parse(&path) {
			Ok(graph) => {
				execute_graph(&graph, tx_task, rx_result);
		}
			Err(msg) =>{panic! (msg);}
		}

	println!("done");
}

fn validate_graph(graph: Graph<BuildTask, ()>) -> Result<Graph<BuildTask, ()>, String> {
	let mut completed:Vec<bool> = vec![];
	let mut queue:Vec<NodeIndex> = vec![];
		graph. each_node(|index: NodeIndex, _:&Node<BuildTask>|->bool {
			completed.push(false);
			queue.push(index);
			true
	});
	let mut count:uint = 0;
	let mut i:uint = 0;
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
	println!("{} {} {}", message.task.working_dir, message.task.exec, message.task.args);
	match Command::new(message.task.exec)
	.args(message.task.args.as_slice())
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

fn execute_graph(graph: &Graph<BuildTask, ()>, tx_task: Sender<TaskMessage>, rx_result: Receiver<ResultMessage>) {
	let mut completed:Vec<bool> = vec![];
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
	let mut count:uint = 0;
	for message in rx_result.iter() {
		assert!(!completed[message.index.node_id()]);
		println!("R: {}", message);
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
		count += 1;
		if count ==completed.len() {
			break;
		}
	}
		free(tx_task);

	for message in rx_result.iter() {
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

fn parse_command_line(args: Vec<String>) -> Vec<String> {
	let mut result: Vec<String> = Vec::new();
	for arg in args.slice(1, args.len()).iter() {
			result.push(arg.clone());
	}
	result
}

struct BuildTask {
title: String,
exec: String,
args: Vec<String>,
working_dir: String,
}

impl Clone for BuildTask {
fn clone(& self) -> BuildTask {
	BuildTask {
	title: self.title.clone(),
	exec: self.exec.clone(),
	args: self.args.clone(),
	working_dir: self .working_dir.clone(),
	}
}
}

struct XgTask {
id: Option<String>,
title: Option<String>,
tool: String,
working_dir: String,
depends_on: Vec<String>,
}

impl fmt::Show for XgTask {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "id={}, title={}, tool={}, working_dir={}, depends_on={}", self .id, self .title, self .tool, self .working_dir, self .depends_on)
}
}

struct XgTool {
id: String,
exec: String,
args: String,
output: Option<String>,
}

impl fmt::Show for XgTool {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "id={}, exec={}", self .id, self .exec)
}
}

fn xg_parse(path: &Path) -> Result<Graph<BuildTask, ()>, String> {
	let file = File::open(path).unwrap();
	let reader = BufferedReader::new(file);

	let mut parser = EventReader::new(reader);
	let mut tasks:Vec<XgTask> = vec![];
	let mut tools:HashMap<String, XgTool> = HashMap::new();
	for e in parser.events() {
		match e {
				XmlEvent::StartElement {name, attributes, ..} => {
				match name.local_name.as_slice() {
						"Task" =>
						{
							match xg_parse_task(&attributes) {
									Ok(task) =>
									{
											tasks.push(task);
									}
									Err(msg) =>
									{
										return Err(msg);
									}
								};
						}
						"Tool" =>
						{
							match xg_parse_tool(&attributes) {
									Ok(tool) =>
									{
											tools.insert(tool.id.to_string(), tool);
									}
									Err(msg) =>
									{
										return Err(msg);
									}
								};
						}
						_ => {}
					}
			}
				XmlEvent::EndElement{..} => {
			}
				_ => {
			}
			}
	}
	xg_parse_create_graph(&tasks, &tools)
}

fn xg_parse_create_graph(tasks:&Vec<XgTask>, tools:&HashMap<String, XgTool>) -> Result<Graph<BuildTask, ()>, String> {
	let mut graph: Graph<BuildTask, ()> = Graph::new();
	let mut nodes: Vec<NodeIndex> = vec![];
	let mut task_refs: HashMap<&str, NodeIndex> = HashMap::new();
	for task in tasks.iter() {
		match tools.get(task.tool.as_slice()){
				Some(tool) => {
				let node = graph.add_node(BuildTask {
				title: match task.title {
						Some(ref v) => {v.clone()}
						_ => {
						match tool.output {
								Some(ref v) => {v.clone()}
								_ => "".to_string()
							}
					}
					},
				exec: tool.exec.clone(),
				args: cmd_parse(tool.args.as_slice()),
				working_dir : task.working_dir.clone(),
				});
				match task.id {
						Some(ref v) => {
							task_refs.insert(v.as_slice(), node);
					}
						_ => {}
					}
				nodes.push(node);
			}
				_ => {
				return Err(format!("Can't find tool with id: {}", task.tool));
			}
			}
	}
	for idx in range(0, nodes.len()) {
		let ref task = tasks[idx];
		let ref node = nodes[idx];
		for id in task.depends_on.iter() {
			let dep_node = task_refs.get(id.as_slice());
			match dep_node {
					Some(v) => {
						graph.add_edge(*node, *v, ());
				}
					_ => {
					return Err(format!("Can't find task for dependency with id: {}", id));
				}
				}
		}
	}
	validate_graph(graph)
}

fn map_attributes (attributes: &Vec<xml::attribute::OwnedAttribute>) -> HashMap< String, String> {
	let mut attrs: HashMap<String, String> = HashMap::new();
	for attr in attributes.iter() {
			attrs.insert(attr.name.local_name.clone(), attr.value.clone());
	}
	attrs
}

fn xg_parse_task (attributes: & Vec<xml::attribute::OwnedAttribute>)->Result<XgTask, String> {
	let mut attrs = map_attributes(attributes);
	// Tool
	let tool: String;
	match attrs.remove("Tool") {
			Some(v) => {tool = v;}
			_ => {return Err("Invalid task data: attribute @Tool not found.".to_string());}
		}
	// WorkingDir
	let working_dir: String;
	match attrs.remove("WorkingDir") {
			Some(v) => {working_dir = v;}
			_ => {return Err("Invalid task data: attribute @WorkingDir not found.".to_string());}
		}
	// DependsOn
	let mut depends_on : Vec<String> = vec![];
	match attrs.remove("DependsOn") {
			Some(v) =>
			{
				for item in v.split_str(";").collect::<Vec<&str>>().iter() {
						depends_on.push(item.to_string())
				}
			}
			_ =>
			{
			}
		};

		Ok(XgTask {
	id: attrs.remove("Name"),
	title: attrs.remove("Caption"),
	tool: tool,
	working_dir: working_dir,
	depends_on: depends_on,
	})
}

fn xg_parse_tool (attributes: &Vec<xml::attribute::OwnedAttribute>)->Result<XgTool, String> {
	let mut attrs = map_attributes(attributes);
	// Name
	let id: String;
	match attrs.remove("Name") {
			Some(v) => {id = v;}
			_ => {return Err("Invalid task data: attribute @Name not found.".to_string());}
		}
	// Path
	let exec: String;
	match attrs.remove("Path") {
			Some(v) => {exec = v;}
			_ => {return Err("Invalid task data: attribute @Name not found.".to_string());}
		}

	Ok(XgTool {
	id: id,
	exec: exec,
	output: attrs.remove("OutputPrefix"),
	args: match attrs.remove("Params") {
			Some(v) => {v}
			_ => {"".to_string()}
		},
	})
}

fn cmd_parse(cmd: &str) -> Vec<String> {
	let mut args: Vec<String> = vec![];
	let mut arg: String = "".to_string();
	let mut escape = false;
	let mut quote = false;
	let mut data = false;
	for c in cmd.chars() {
		match c {
				' ' | '\t' => {
				if escape {
						arg.push('\\');
						escape = false;
				}
				if quote {
						arg.push(c);
						data = true;
				} else if data {
						args.push(arg);
						arg = "".to_string();
						data = false;
				}
			}
				'\\' => {
				if escape {
						arg.push(c);
				}
				data = true;
				escape = !escape;
			}
				'"' => {
				if escape {
						arg.push(c);
						escape = false;
				} else {
					quote = !quote;
				}
				data = true;
			}
				_ => {
				if escape {
						arg.push('\\');
						escape = false;
				}
				arg.push(c);
				data = true;
			}
			}
	}
	if data {
			args.push(arg);
	}
	return args;
}

#[test]
fn test_cmd_parse_1() {
	assert_eq!(cmd_parse("\"abc\" d e"), ["abc", "d", "e"]);
}

#[test]
fn test_cmd_parse_2() {
	assert_eq!(cmd_parse(" \"abc\" d e "), ["abc", "d", "e"]);
}

#[test]
fn test_cmd_parse_3() {
	assert_eq!(cmd_parse("\"\" \"abc\" d e \"\""), ["", "abc", "d", "e", ""]);
}

#[test]
fn test_cmd_parse_4() {
	assert_eq!(cmd_parse("a\\\\\\\\b d\"e f\"g h"), ["a\\\\b", "de fg", "h"]);
}

#[test]
fn test_cmd_parse_5() {
	assert_eq!(cmd_parse("a\\\\\\\"b c d"), ["a\\\"b", "c", "d"]);
}

#[test]
fn test_cmd_parse_6() {
	assert_eq!(cmd_parse("a\\\\\\\\\"b c\" d e"), ["a\\\\b c", "d", "e"]);
}

#[test]
fn test_cmd_parse_7() {
	assert_eq!(cmd_parse("C:\\Windows\\System32 d e"), ["C:\\Windows\\System32", "d", "e"]);
}
