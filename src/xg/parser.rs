extern crate xml;
extern crate petgraph;

use common::{BuildTask};
use wincmd;

use std::fmt::{Display, Formatter};
use std::io::{Read, Error, ErrorKind};
use std::collections::{HashSet, HashMap};
use std::iter::FromIterator;

use self::petgraph::graph::{Graph, NodeIndex};

use self::xml::reader::EventReader;
use self::xml::reader::events::XmlEvent;

#[derive(Debug)]
pub enum XgParseError {
	AttributeNotFound(&'static str),
	ToolNotFound(String),
	DependencyNotFound(String),
}
				
impl Display for XgParseError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&XgParseError::AttributeNotFound(ref attr) => write!(f, "attribute not found: {}", attr),
			&XgParseError::ToolNotFound(ref id) => write!(f, "сan't find tool with id: {}", id),
			&XgParseError::DependencyNotFound(ref id) => write!(f, "сan't find task for dependency with id: {}", id),
		}
	}
}

impl ::std::error::Error for XgParseError {
	fn description(&self) -> &str {
		match self {
			&XgParseError::AttributeNotFound(_) => "attribute not found",
			&XgParseError::ToolNotFound(_) => "сan't find tool with id",
			&XgParseError::DependencyNotFound(_) => "сan't find task for dependency with id",
		}
	}

	fn cause(&self) -> Option<&::std::error::Error> {
		None
	}
}

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
				match &name.local_name[..] {
					"Task" => {
						tasks.push(try! (parse_task (attributes)));
					}
					"Tool" => {
						let tool = try! (parse_tool (attributes));
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
				return Err(Error::new(ErrorKind::InvalidInput, XgParseError::ToolNotFound(task.tool.clone())))
			}
		}
	}
	for idx in 0..nodes.len() {
		let ref task = tasks[idx];
		let ref node = nodes[idx];
		for id in task.depends_on.iter() {
			match task_refs.get(&id[..]) {
				Some(v) => {
					graph.add_edge(*node, *v, ());
				}
				_ => {
					return Err(Error::new(ErrorKind::InvalidInput, XgParseError::DependencyNotFound(task.tool.clone())))
				}
			}
		}
	}
	Ok(graph)
}

fn map_attributes (attributes: Vec<xml::attribute::OwnedAttribute>) -> HashMap<String, String> {
	HashMap::from_iter(attributes.into_iter().map(|v| (v.name.local_name, v.value)))
}

fn parse_task (attributes: Vec<xml::attribute::OwnedAttribute>)->Result<XgTask, Error> {
	let mut attrs = map_attributes(attributes);
	let tool = try! (take_attr(&mut attrs, "Tool"));
	let working_dir = try! (take_attr(&mut attrs, "WorkingDir"));
	// DependsOn
	let depends_on : HashSet<String> = match attrs.remove("DependsOn") {
		Some(v) => HashSet::from_iter(v.split(";").map(|v| v.to_string())),
		_ => HashSet::new()
	};

	Ok(XgTask {
		id: attrs.remove("Name"),
		title: attrs.remove("Caption"),
		tool: tool,
		working_dir: working_dir,
		depends_on: depends_on.into_iter().collect::<Vec<String>>(),
	})
}

fn parse_tool (attributes: Vec<xml::attribute::OwnedAttribute>)->Result<XgTool, Error> {
	let mut attrs = map_attributes(attributes);
	let id = try! (take_attr(&mut attrs, "Name"));
	let exec = try! (take_attr(&mut attrs, "Path"));
	Ok(XgTool {
		id: id,
		exec: exec,
		output: attrs.remove("OutputPrefix"),
		args: match attrs.remove("Params") {
			Some(v) => v,
			_ => String::new()
		},
	})
}

fn take_attr(attrs: &mut HashMap<String, String>, attr: &'static str) -> Result<String, Error> {
	match attrs.remove(attr) {
		Some(v) => Ok(v),
		_ => Err(Error::new(ErrorKind::InvalidInput, XgParseError::AttributeNotFound(attr)))
	}
}
