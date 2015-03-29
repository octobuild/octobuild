extern crate xml;

use common::{BuildTask};
use graph::{Graph, NodeIndex};
use wincmd;

use std::io::{Read, Error, ErrorKind};
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

pub fn parse<B: Read>(reader: B) -> Result<Graph<BuildTask, ()>, Error> {
	let mut parser = EventReader::new(reader);
	let mut tasks:Vec<XgTask> = Vec::new();
	let mut tools:HashMap<String, XgTool> = HashMap::new();
	for e in parser.events() {
		match e {
			XmlEvent::StartElement {name, attributes, ..} => {
				match name.local_name.as_slice() {
					"Task" => {
						tasks.push(try! (parse_task (&attributes)));
					}
					"Tool" => {
						let tool = try! (parse_tool (&attributes));
						tools.insert(tool.id.to_string(), tool);
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

fn parse_create_graph(tasks:&Vec<XgTask>, tools:&HashMap<String, XgTool>) -> Result<Graph<BuildTask, ()>, Error> {
	let mut graph: Graph<BuildTask, ()> = Graph::new();
	let mut nodes: Vec<NodeIndex> = Vec::new();
	let mut task_refs: HashMap<&str, NodeIndex> = HashMap::new();
	for task in tasks.iter() {
		match tools.get(&task.tool){
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
					args: wincmd::parse(&tool.args),
					working_dir : task.working_dir.clone(),
				});
				match task.id {
					Some(ref v) => {
						task_refs.insert(&v, node);
					}
					_ => {}
				}
				nodes.push(node);
			}
			_ => {
				return Err(Error::new(ErrorKind::InvalidInput, "Can't find tool with id: {}", Some(task.tool.clone())));
			}
		}
	}
	for idx in 0..nodes.len() {
		let ref task = tasks[idx];
		let ref node = nodes[idx];
		for id in task.depends_on.iter() {
			let dep_node = task_refs.get(id.as_slice());
			match dep_node {
				Some(v) => {
					graph.add_edge(*node, *v, ());
				}
				_ => {
					return Err(Error::new(ErrorKind::InvalidInput, "Can't find task for dependency with id: {}", Some(id.clone())));
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

fn parse_task (attributes: & Vec<xml::attribute::OwnedAttribute>)->Result<XgTask, Error> {
	let mut attrs = map_attributes(attributes);
	// Tool
	let tool = match attrs.remove("Tool") {
		Some(v) => v,
		_ => {
			return Err(Error::new(ErrorKind::InvalidInput, "Invalid task data: attribute @Tool not found", None));
		}
	};
	// WorkingDir
	let working_dir = match attrs.remove("WorkingDir") {
		Some(v) => v,
		_ => {
			return Err(Error::new(ErrorKind::InvalidInput, "Invalid task data: attribute @WorkingDir not found", None));
		}
	};
	// DependsOn
	let mut depends_on : Vec<String> = Vec::new();
	match attrs.remove("DependsOn") {
		Some(v) => {
			for item in v.split(";").collect::<Vec<&str>>().iter() {
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

fn parse_tool (attributes: &Vec<xml::attribute::OwnedAttribute>)->Result<XgTool, Error> {
	let mut attrs = map_attributes(attributes);
	// Name
	let id = match attrs.remove("Name") {
		Some(v) => v,
		_ => {
			return Err(Error::new(ErrorKind::InvalidInput, "Invalid task data: attribute @Name not found", None));
		}
	};
	// Path
	let exec = match attrs.remove("Path") {
		Some(v) => v,
		_ => {
			return Err(Error::new(ErrorKind::InvalidInput, "Invalid task data: attribute @Path not found", None));
		}
	};
	
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
