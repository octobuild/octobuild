extern crate xml;

use common::{BuildTask};
use graph::{Graph, NodeIndex};
use wincmd;

use std::old_io::Buffer;
use std::collections::HashMap;

use self::xml::reader::EventReader;
use self::xml::reader::events::XmlEvent;

#[derive(Debug)]
struct XgTask {
	id: Option<String>,
	title: Option<String>,
	tool: String,
	working_dir: String,
	depends_on: Vec<String>,
}

#[derive(Debug)]
struct XgTool {
	id: String,
	exec: String,
	args: String,
	output: Option<String>,
}

pub fn parse<B: Buffer>(reader: B) -> Result<Graph<BuildTask, ()>, String> {
	let mut parser = EventReader::new(reader);
	let mut tasks:Vec<XgTask> = Vec::new();
	let mut tools:HashMap<String, XgTool> = HashMap::new();
	for e in parser.events() {
		match e {
			XmlEvent::StartElement {name, attributes, ..} => {
				match name.local_name.as_slice() {
					"Task" => {
						match parse_task(&attributes) {
							Ok(task) => {
								tasks.push(task);
							}
							Err(msg) => {
								return Err(msg);
							}
						};
					}
					"Tool" => {
						match parse_tool(&attributes) {
							Ok(tool) => {
								tools.insert(tool.id.to_string(), tool);
							}
							Err(msg) => {
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
	parse_create_graph(&tasks, &tools)
}

fn parse_create_graph(tasks:&Vec<XgTask>, tools:&HashMap<String, XgTool>) -> Result<Graph<BuildTask, ()>, String> {
	let mut graph: Graph<BuildTask, ()> = Graph::new();
	let mut nodes: Vec<NodeIndex> = Vec::new();
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
								_ => String::new()
							}
						}
					},
					exec: tool.exec.clone(),
					args: wincmd::parse(tool.args.as_slice()),
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
	Ok(graph)
}

fn map_attributes (attributes: &Vec<xml::attribute::OwnedAttribute>) -> HashMap< String, String> {
	let mut attrs: HashMap<String, String> = HashMap::new();
	for attr in attributes.iter() {
		attrs.insert(attr.name.local_name.clone(), attr.value.clone());
	}
	attrs
}

fn parse_task (attributes: & Vec<xml::attribute::OwnedAttribute>)->Result<XgTask, String> {
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
	let mut depends_on : Vec<String> = Vec::new();
	match attrs.remove("DependsOn") {
		Some(v) => {
			for item in v.split_str(";").collect::<Vec<&str>>().iter() {
				depends_on.push(item.to_string())
			}
		}
		_ => {
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

fn parse_tool (attributes: &Vec<xml::attribute::OwnedAttribute>)->Result<XgTool, String> {
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
			_ => {String::new()}
		},
	})
}
