extern crate xml;
extern crate petgraph;

use common::{BuildTask};
use cmd;

use std::fmt::{Display, Formatter};
use std::io::{Read, Error, ErrorKind};
use std::collections::{HashSet, HashMap};
use std::iter::FromIterator;
use std::sync::Arc;

use self::petgraph::graph::{Graph, NodeIndex};

use self::xml::reader::EventReader;
use self::xml::reader::XmlEvent;

#[derive(Debug)]
pub enum XgParseError {
	AttributeNotFound(&'static str),
	EnvironmentNotFound(String),
	ToolNotFound(String),
	DependencyNotFound(String),
	InvalidStreamFormat,
	EndOfStream,
	XmlError(self::xml::reader::Error),
}
				
impl Display for XgParseError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&XgParseError::AttributeNotFound(ref attr) => write!(f, "attribute not found: {}", attr),
			&XgParseError::EnvironmentNotFound(ref id) => write!(f, "сan't find environment with id: {}", id),
			&XgParseError::ToolNotFound(ref id) => write!(f, "сan't find tool with id: {}", id),
			&XgParseError::DependencyNotFound(ref id) => write!(f, "сan't find task for dependency with id: {}", id),
			&XgParseError::InvalidStreamFormat => write!(f, "unexpected XML-stream root element"),
			&XgParseError::EndOfStream => write!(f, "unexpended end of stream"),
			&XgParseError::XmlError(ref e) => write!(f, "xml reading error: {}", e),
		}
	}
}

impl ::std::error::Error for XgParseError {
	fn description(&self) -> &str {
		match self {
			&XgParseError::AttributeNotFound(_) => "attribute not found",
			&XgParseError::EnvironmentNotFound(_) => "сan't find environment by id",
			&XgParseError::ToolNotFound(_) => "сan't find tool by id",
			&XgParseError::DependencyNotFound(_) => "сan't find task for dependency by id",
			&XgParseError::InvalidStreamFormat => "unexpected XML-stream root element",
			&XgParseError::EndOfStream => "unexpended end of stream",
			&XgParseError::XmlError(_) => "xml reading error",
		}
	}

	fn cause(&self) -> Option<&::std::error::Error> {
		None
	}
}

#[derive(Debug)]
struct XgEnvironment {
    variables: Arc<HashMap<String, String>>,
    tools: HashMap<String, XgTool>,
}

#[derive(Debug)]
struct XgProject {
	env: String,
	tasks: HashMap<String, XgTask>,
}

#[derive(Debug)]
struct XgTask {
	title: Option<String>,
	tool: String,
	working_dir: String,
	depends_on: Vec<String>,
}

#[derive(Debug)]
struct XgTool {
	exec: String,
	args: String,
	output: Option<String>,
}

pub fn parse<R: Read>(reader: R) -> Result<Graph<BuildTask, ()>, Error> {
	let mut parser = EventReader::new(reader);
	loop {
		match try! (next_xml_event(&mut parser)) {
			XmlEvent::StartElement {name, ..} => {
				return match &name.local_name[..] {
					"BuildSet" => parse_build_set(&mut parser),
					_ => Err(Error::new(ErrorKind::InvalidInput, XgParseError::InvalidStreamFormat)),
				}
			}
			_ => {}
		}
	}
}

pub fn parse_build_set<R: Read>(events: &mut EventReader<R>) -> Result<Graph<BuildTask, ()>, Error> {
	let mut envs:HashMap<String, XgEnvironment> = HashMap::new();
	let mut projects:Vec<XgProject> = Vec::new();
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, attributes, ..} => {
				match &name.local_name[..] {
					"Environments" => {try! (parse_environments(events, &mut envs));}
					"Project" => {
						let mut attrs = map_attributes(attributes);
						projects.push(XgProject {
							env: try! (take_attr(&mut attrs, "Env")),
							tasks: try! (parse_tasks(events)),
						});
					}
					_ => {try! (parse_skip(events, ()));}
				}
			}
			XmlEvent::EndElement {..} => {break;}
			_ => {}
		}
	}
	parse_create_graph(envs, projects)
}

fn parse_environments<R: Read>(events: &mut EventReader<R>, envs: &mut HashMap<String, XgEnvironment>) -> Result<(), Error> {
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, attributes, ..} => {
				match &name.local_name[..] {
					"Environment" => {
						let mut attrs = map_attributes(attributes);
						let name = try! (take_attr(&mut attrs, "Name"));
						envs.insert(name, try!(parse_environment(events)));
					}
					_ => {try!(parse_skip(events, ()));}
				}
			}			
			XmlEvent::EndElement {..} => {return Ok(());}
			_ => {}
		}
	}
}

fn parse_environment<R: Read>(events: &mut EventReader<R>) -> Result<XgEnvironment, Error> {
	let mut variables = HashMap::new();
	let mut tools = HashMap::new();
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, ..} => {
				match &name.local_name[..] {
					"Variables" => try!(parse_variables(events, &mut variables)),
					"Tools" => try!(parse_tools(events, &mut tools)),
					_ => try!(parse_skip(events, ())),
				};
			}			
			XmlEvent::EndElement {..} => {break;}
			_ => {}
		}
	}
	Ok(XgEnvironment {
		variables: Arc::new(variables),
		tools: tools,
	})
}

fn parse_variables<R: Read>(events: &mut EventReader<R>, variables: &mut HashMap<String, String>) -> Result<(), Error> {
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, attributes, ..} => {
				match &name.local_name[..] {
					"Variable" => {
						let mut attrs = map_attributes(attributes);
						let name = try! (take_attr(&mut attrs, "Name"));
						let value = try! (take_attr(&mut attrs, "Value"));
						variables.insert(name, value);
					}
					_ => {
					}
				}
				try!(parse_skip(events, ()));
			}
			XmlEvent::EndElement {..} => {
				return Ok(());
			}
			_ => {}
		}
	}
}

fn parse_tools<R: Read>(events: &mut EventReader<R>, tools: &mut HashMap<String, XgTool>) -> Result<(), Error> {
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, attributes, ..} => {
				match &name.local_name[..] {
					"Tool" => {
						let mut attrs = map_attributes(attributes);
						let name = try! (take_attr(&mut attrs, "Name"));
						let exec = try! (take_attr(&mut attrs, "Path"));
						tools.insert(name, XgTool {
							exec: exec,
							output: attrs.remove("OutputPrefix"),
							args: attrs.remove("Params").unwrap_or_else(|| String::new()),
						});
					}
					_ => {
					}
				}
				try!(parse_skip(events, ()));
			}
			XmlEvent::EndElement {..} => {
				return Ok(());
			}
			_ => {}
		}
	}
}

fn parse_tasks<R: Read>(events: &mut EventReader<R>) -> Result<HashMap<String, XgTask>, Error> {
	let mut tasks = HashMap::new();
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {name, attributes, ..} => {
				match &name.local_name[..] {
					"Task" => {
						let mut attrs = map_attributes(attributes);
						let name = try! (take_attr(&mut attrs, "Name"));
						let tool = try! (take_attr(&mut attrs, "Tool"));
						let working_dir = try! (take_attr(&mut attrs, "WorkingDir"));
						// DependsOn
						let depends_on : HashSet<String> = match attrs.remove("DependsOn") {
							Some(v) => HashSet::from_iter(v.split(";").map(|v| v.to_string())),
							_ => HashSet::new()
						};

						tasks.insert(name.clone(), XgTask {
							title: attrs.remove("Caption"),
							tool: tool,
							working_dir: working_dir,
							depends_on: depends_on.into_iter().collect::<Vec<String>>(),
						});
					}
					_ => {
					}
				}
				try!(parse_skip(events, ()));
			}
			XmlEvent::EndElement {..} => {
				return Ok(tasks);
			}
			_ => {}
		}
	}
}

fn parse_skip<R: Read, T>(events: &mut EventReader<R>, result: T) -> Result<T, Error> {
	let mut depth: isize = 0;
	loop {
		match try! (next_xml_event(events)) {
			XmlEvent::StartElement {..} => {
				depth += 1;
			}
			XmlEvent::EndElement {..} => {
				if depth == 0 {break;}
				depth -= 1;
			}
			_=> {}
		}
	}
	Ok(result)
}

fn next_xml_event<R: Read>(reader: &mut EventReader<R>) -> Result<XmlEvent, Error> {
	reader.next().map_err(|e| Error::new(ErrorKind::InvalidInput, XgParseError::XmlError(e)))
}

fn parse_create_graph(envs:HashMap<String, XgEnvironment>, projects:Vec<XgProject>) -> Result<Graph<BuildTask, ()>, Error> {
	let mut graph: Graph<BuildTask, ()> = Graph::new();
	for project in projects.into_iter() {
		let env = try!(envs.get(&project.env).ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::EnvironmentNotFound(project.env.clone()))));
		try!(graph_project(&mut graph, project, env));
	}
	Ok(graph)
}

fn graph_project(graph: &mut Graph<BuildTask, ()>, project: XgProject, env: &XgEnvironment) -> Result<(), Error> {
	let mut nodes: Vec<NodeIndex> = Vec::new();
	let mut task_refs: HashMap<&str, NodeIndex> = HashMap::new();
	for (id, task) in project.tasks.iter() {
		let tool = try!(env.tools.get(&task.tool).ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::ToolNotFound(task.tool.clone()))));
		let node = graph.add_node(BuildTask {
			title: match task.title {
				Some(ref v) => v.clone(),
				None => match tool.output {
					Some(ref v) => v.clone(),
					None => String::new(),
				},
			},
			exec: tool.exec.clone(),
			args: try! (cmd::native::parse(&tool.args)),
			env: env.variables.clone(),
			working_dir : task.working_dir.clone(),
		});
		task_refs.insert(&id, node);
		nodes.push(node);
	}
	for (src_id, task) in project.tasks.iter() {
		let src = task_refs.get(&src_id[..]).unwrap();
		for dst_id in task.depends_on.iter() {
			let dst = try!(task_refs.get(&dst_id[..]).ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::DependencyNotFound(dst_id.clone()))));
			graph.add_edge(*src, *dst, ());
		}
	}
	Ok(())
}

fn map_attributes (attributes: Vec<xml::attribute::OwnedAttribute>) -> HashMap<String, String> {
	HashMap::from_iter(attributes.into_iter().map(|v| (v.name.local_name, v.value)))
}

fn take_attr(attrs: &mut HashMap<String, String>, attr: &'static str) -> Result<String, Error> {
	attrs.remove(attr).ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::AttributeNotFound(attr)))
}

#[test]
fn test_parse_smoke() {
	use std::fs::File;
	use std::io::BufReader;

	parse(BufReader::new(File::open("tests/graph-parser.xml").unwrap())).unwrap();
}
