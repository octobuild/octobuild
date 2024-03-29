use std::collections::{HashMap, HashSet};
use std::env;
use std::io::{Error, ErrorKind, Read};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use thiserror::Error;

use crate::compiler::{CommandEnv, CommandInfo};

use petgraph::graph::{Graph, NodeIndex};
use xml::reader::EventReader;
use xml::reader::XmlEvent;

#[derive(Debug)]
pub struct XgNode {
    pub title: String,
    pub command: CommandInfo,
    pub raw_args: Rc<String>,
}

pub type XgGraph = Graph<XgNode, ()>;

#[derive(Error, Debug)]
enum XgParseError {
    #[error("attribute not found: {0}")]
    AttributeNotFound(&'static str),
    #[error("сan't find environment with id: {0}")]
    EnvironmentNotFound(String),
    #[error("сan't find tool with id: {0}")]
    ToolNotFound(String),
    #[error("сan't find task for dependency with id: {0}")]
    DependencyNotFound(String),
    #[error("unexpected XML-stream root element")]
    InvalidStreamFormat,
    #[error("xml reading error: {0}")]
    XmlError(xml::reader::Error),
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
    args: Rc<String>,
    output: Option<String>,
}

pub fn parse<R: Read>(graph: &mut XgGraph, reader: R) -> Result<(), Error> {
    let mut parser = EventReader::new(reader);
    loop {
        if let XmlEvent::StartElement { name, .. } = next_xml_event(&mut parser)? {
            return match &name.local_name[..] {
                "BuildSet" => parse_build_set(graph, &mut parser),
                _ => Err(Error::new(
                    ErrorKind::InvalidInput,
                    XgParseError::InvalidStreamFormat,
                )),
            };
        }
    }
}

fn parse_build_set<R: Read>(graph: &mut XgGraph, events: &mut EventReader<R>) -> Result<(), Error> {
    let mut envs: HashMap<String, XgEnvironment> = HashMap::new();
    let mut projects: Vec<XgProject> = Vec::new();
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => match &name.local_name[..] {
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
    parse_create_graph(graph, &envs, &projects)
}

fn parse_environments<R: Read>(
    events: &mut EventReader<R>,
    envs: &mut HashMap<String, XgEnvironment>,
) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => match &name.local_name[..] {
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

fn parse_variables<R: Read>(
    events: &mut EventReader<R>,
    variables: &mut CommandEnv,
) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.local_name == "Variable" {
                    let mut attrs = map_attributes(attributes);
                    let name = take_attr(&mut attrs, "Name")?;
                    let value = take_attr(&mut attrs, "Value")?;
                    variables.insert(name, value);
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

fn parse_tools<R: Read>(
    events: &mut EventReader<R>,
    tools: &mut HashMap<String, XgTool>,
) -> Result<(), Error> {
    loop {
        match next_xml_event(events)? {
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.local_name == "Tool" {
                    let mut attrs = map_attributes(attributes);
                    let name = take_attr(&mut attrs, "Name")?;
                    let exec = take_attr(&mut attrs, "Path")?;
                    tools.insert(
                        name,
                        XgTool {
                            exec: PathBuf::from(&exec),
                            output: attrs.remove("OutputPrefix"),
                            args: Rc::new(attrs.remove("Params").unwrap_or_default()),
                        },
                    );
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
            XmlEvent::StartElement {
                name, attributes, ..
            } => {
                if name.local_name == "Task" {
                    let mut attrs = map_attributes(attributes);
                    let name = take_attr(&mut attrs, "Name")?;
                    let tool = take_attr(&mut attrs, "Tool")?;
                    let working_dir = take_attr(&mut attrs, "WorkingDir")?;
                    // DependsOn
                    let depends_on: HashSet<String> = match attrs.remove("DependsOn") {
                        Some(v) => v.split(';').map(ToString::to_string).collect(),
                        _ => HashSet::new(),
                    };

                    tasks.insert(
                        name.clone(),
                        XgTask {
                            title: attrs.remove("Caption"),
                            tool,
                            working_dir: PathBuf::from(&working_dir),
                            depends_on: depends_on.into_iter().collect::<Vec<String>>(),
                        },
                    );
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
    envs: &HashMap<String, XgEnvironment>,
    projects: &Vec<XgProject>,
) -> Result<(), Error> {
    for project in projects {
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

fn graph_project(
    graph: &mut XgGraph,
    project: &XgProject,
    env: &XgEnvironment,
) -> Result<(), Error> {
    let mut nodes: Vec<NodeIndex> = Vec::new();
    let mut task_refs: HashMap<&str, NodeIndex> = HashMap::new();
    for (id, task) in &project.tasks {
        let tool = env.tools.get(&task.tool).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                XgParseError::ToolNotFound(task.tool.clone()),
            )
        })?;
        let node = graph.add_node(XgNode {
            title: task.title.as_ref().map_or_else(
                || tool.output.as_ref().map_or_else(String::new, |v| v.clone()),
                |v| v.clone(),
            ),
            command: CommandInfo {
                program: tool.exec.clone(),
                // Working directory
                current_dir: Some(task.working_dir.clone()),
                // Environment variables
                env: env.variables.clone(),
            },
            raw_args: tool.args.clone(),
        });
        task_refs.insert(id, node);
        nodes.push(node);
    }
    for (src_id, task) in &project.tasks {
        let src = task_refs.get(&src_id[..]).unwrap();
        for dst_id in &task.depends_on {
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
    attributes
        .into_iter()
        .map(|v| (v.name.local_name, v.value))
        .collect()
}

fn take_attr(attrs: &mut HashMap<String, String>, attr: &'static str) -> Result<String, Error> {
    attrs.remove(attr).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            XgParseError::AttributeNotFound(attr),
        )
    })
}
