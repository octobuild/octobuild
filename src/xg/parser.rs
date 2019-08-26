extern crate petgraph;
extern crate xml;

use cmd;
use compiler::{CommandEnv, CommandInfo};

use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::{Display, Formatter};
use std::io::{Error, ErrorKind, Read};
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use self::petgraph::graph::{Graph, NodeIndex};

use self::xml::reader::EventReader;
use self::xml::reader::XmlEvent;

#[derive(Debug)]
pub struct XgNode {
    pub title: String,
    pub command: CommandInfo,
    pub args: Vec<String>,
}

pub type XgGraph = Graph<XgNode, ()>;

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
    variables: Arc<CommandEnv>,
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
    working_dir: PathBuf,
    depends_on: Vec<String>,
}

#[derive(Debug)]
struct XgTool {
    exec: PathBuf,
    args: String,
    output: Option<String>,
}

pub fn parse<R: Read>(graph: &mut XgGraph, reader: R) -> Result<(), Error> {
    let mut parser = EventReader::new(reader);
    loop {
        match next_xml_event(&mut parser)? {
            XmlEvent::StartElement { name, .. } => {
                return match &name.local_name[..] {
                    "BuildSet" => parse_build_set(graph, &mut parser),
                    _ => Err(Error::new(ErrorKind::InvalidInput, XgParseError::InvalidStreamFormat)),
                }
            }
            _ => {}
        }
    }
}

pub fn parse_build_set<R: Read>(graph: &mut XgGraph, events: &mut EventReader<R>) -> Result<(), Error> {
    let mut envs: HashMap<String, XgEnvironment> = HashMap::new();
    let mut projects: Vec<XgProject> = Vec::new();
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, attributes, .. } => match &name.local_name[..] {
                "Environments" => {
                    parse_environments(events, &mut envs)?;
                }
                "Project" => {
                    let mut attrs = map_attributes(attributes);
                    projects.push(XgProject {
                        env: take_attr(&mut attrs, "Env")?,
                        tasks: parse_tasks(events)?,
                    });
                }
                _ => {
                    parse_skip(events, ())?;
                }
            },
            XmlEvent::EndElement { .. } => {
                break;
            }
            _ => {}
        }
    }
    parse_create_graph(graph, envs, projects)
}

fn parse_environments<R: Read>(
    events: &mut EventReader<R>,
    envs: &mut HashMap<String, XgEnvironment>,
) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, attributes, .. } => match &name.local_name[..] {
                "Environment" => {
                    let mut attrs = map_attributes(attributes);
                    let name = take_attr(&mut attrs, "Name")?;
                    envs.insert(name, parse_environment(events)?);
                }
                _ => {
                    parse_skip(events, ())?;
                }
            },
            XmlEvent::EndElement { .. } => {
                return Ok(());
            }
            _ => {}
        }
    }
}

fn parse_environment<R: Read>(events: &mut EventReader<R>) -> Result<XgEnvironment, Error> {
    let mut variables = env::vars().collect();
    let mut tools = HashMap::new();
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, .. } => {
                match &name.local_name[..] {
                    "Variables" => parse_variables(events, &mut variables)?,
                    "Tools" => parse_tools(events, &mut tools)?,
                    _ => parse_skip(events, ())?,
                };
            }
            XmlEvent::EndElement { .. } => {
                break;
            }
            _ => {}
        }
    }
    Ok(XgEnvironment {
        variables: Arc::new(variables),
        tools,
    })
}

fn parse_variables<R: Read>(events: &mut EventReader<R>, variables: &mut CommandEnv) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, attributes, .. } => {
                match &name.local_name[..] {
                    "Variable" => {
                        let mut attrs = map_attributes(attributes);
                        let name = take_attr(&mut attrs, "Name")?;
                        let value = take_attr(&mut attrs, "Value")?;
                        variables.insert(name, value);
                    }
                    _ => {}
                }
                parse_skip(events, ())?;
            }
            XmlEvent::EndElement { .. } => {
                return Ok(());
            }
            _ => {}
        }
    }
}

fn parse_tools<R: Read>(events: &mut EventReader<R>, tools: &mut HashMap<String, XgTool>) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, attributes, .. } => {
                match &name.local_name[..] {
                    "Tool" => {
                        let mut attrs = map_attributes(attributes);
                        let name = take_attr(&mut attrs, "Name")?;
                        let exec = take_attr(&mut attrs, "Path")?;
                        tools.insert(
                            name,
                            XgTool {
                                exec: Path::new(&exec).to_path_buf(),
                                output: attrs.remove("OutputPrefix"),
                                args: attrs.remove("Params").unwrap_or_else(|| String::new()),
                            },
                        );
                    }
                    _ => {}
                }
                parse_skip(events, ())?;
            }
            XmlEvent::EndElement { .. } => {
                return Ok(());
            }
            _ => {}
        }
    }
}

fn parse_tasks<R: Read>(events: &mut EventReader<R>) -> Result<HashMap<String, XgTask>, Error> {
    let mut tasks = HashMap::new();
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { name, attributes, .. } => {
                match &name.local_name[..] {
                    "Task" => {
                        let mut attrs = map_attributes(attributes);
                        let name = take_attr(&mut attrs, "Name")?;
                        let tool = take_attr(&mut attrs, "Tool")?;
                        let working_dir = take_attr(&mut attrs, "WorkingDir")?;
                        // DependsOn
                        let depends_on: HashSet<String> = match attrs.remove("DependsOn") {
                            Some(v) => HashSet::from_iter(v.split(";").map(|v| v.to_string())),
                            _ => HashSet::new(),
                        };

                        tasks.insert(
                            name.clone(),
                            XgTask {
                                title: attrs.remove("Caption"),
                                tool,
                                working_dir: Path::new(&working_dir).to_path_buf(),
                                depends_on: depends_on.into_iter().collect::<Vec<String>>(),
                            },
                        );
                    }
                    _ => {}
                }
                parse_skip(events, ())?;
            }
            XmlEvent::EndElement { .. } => {
                return Ok(tasks);
            }
            _ => {}
        }
    }
}

fn parse_skip<R: Read, T>(events: &mut EventReader<R>, result: T) -> Result<T, Error> {
    let mut depth: isize = 0;
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement { .. } => {
                depth += 1;
            }
            XmlEvent::EndElement { .. } => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    Ok(result)
}

fn next_xml_event<R: Read>(reader: &mut EventReader<R>) -> Result<XmlEvent, Error> {
    reader
        .next()
        .map_err(|e| Error::new(ErrorKind::InvalidInput, XgParseError::XmlError(e)))
}

fn parse_create_graph(
    graph: &mut XgGraph,
    envs: HashMap<String, XgEnvironment>,
    projects: Vec<XgProject>,
) -> Result<(), Error> {
    for project in projects.into_iter() {
        let env = envs.get(&project.env).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                XgParseError::EnvironmentNotFound(project.env.clone()),
            )
        })?;
        graph_project(graph, project, env)?;
    }
    Ok(())
}

fn graph_project(graph: &mut XgGraph, project: XgProject, env: &XgEnvironment) -> Result<(), Error> {
    let mut nodes: Vec<NodeIndex> = Vec::new();
    let mut task_refs: HashMap<&str, NodeIndex> = HashMap::new();
    for (id, task) in project.tasks.iter() {
        let tool = env
            .tools
            .get(&task.tool)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::ToolNotFound(task.tool.clone())))?;
        let node = graph.add_node(XgNode {
            title: task.title.as_ref().map_or_else(
                || tool.output.as_ref().map_or_else(|| String::new(), |v| v.clone()),
                |v| v.clone(),
            ),
            command: CommandInfo {
                program: tool.exec.clone(),
                // Working directory
                current_dir: Some(task.working_dir.clone()),
                // Environment variables
                env: env.variables.clone(),
            },
            args: cmd::native::parse(&tool.args)?,
        });
        task_refs.insert(&id, node);
        nodes.push(node);
    }
    for (src_id, task) in project.tasks.iter() {
        let src = task_refs.get(&src_id[..]).unwrap();
        for dst_id in task.depends_on.iter() {
            let dst = task_refs.get(&dst_id[..]).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidInput,
                    XgParseError::DependencyNotFound(dst_id.clone()),
                )
            })?;
            graph.add_edge(*src, *dst, ());
        }
    }
    Ok(())
}

fn map_attributes(attributes: Vec<xml::attribute::OwnedAttribute>) -> HashMap<String, String> {
    HashMap::from_iter(attributes.into_iter().map(|v| (v.name.local_name, v.value)))
}

fn take_attr(attrs: &mut HashMap<String, String>, attr: &'static str) -> Result<String, Error> {
    attrs
        .remove(attr)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, XgParseError::AttributeNotFound(attr)))
}

#[test]
fn test_parse_smoke() {
    use std::fs::File;
    use std::io::BufReader;

    parse(
        &mut Graph::new(),
        BufReader::new(File::open("tests/graph-parser.xml").unwrap()),
    )
    .unwrap();
}
