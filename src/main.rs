extern crate xml;

use std::os;

use std::io::{File, BufferedReader};
use std::fmt;

use xml::reader::EventReader;
use xml::reader::events::XmlEvent;

fn main() {
	println!("XGConsole:");
	for arg in parse_command_line(os::args()).iter() {
		println!("  {}", arg);
	}
	sample();
}

fn parse_command_line(args: Vec<String>) -> Vec<String> {
	let mut result: Vec<String> = Vec::new();
	for arg in args.slice(1, args.len()).iter() {
			result.push(arg.clone());
	}
	result
}

struct XgTask {
id: Option<String>,
title: Option<String>,
tool: Option<String>,
working_dir: Option<String>,
depends_on: Vec<String>,
}

impl XgTask {
fn new() -> XgTask {
	XgTask {
	id: None,
	title: None,
	tool: None,
	working_dir: None,
	depends_on: vec![],
	}
}
}

impl fmt::Show for XgTask {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "id={}, title={}, tool={}, working_dir={}, depends_on={}", self .id, self .title, self .tool, self .working_dir, self .depends_on)
}
}

struct XgTool {
id: Option<String>,
path: Option<String>,
params: Option<String>,
}

impl XgTool {
fn new() -> XgTool {
	XgTool {
	id: None,
	path: None,
	params: None,
	}
}
}

impl fmt::Show for XgTool {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "id={}, path={}", self .id, self .path)
}
}

fn sample() {
	let file = File::open(&Path::new("tests/graph-parser.xml")).unwrap();
	let reader = BufferedReader::new(file);

	let mut parser = EventReader::new(reader);
	let mut tasks:Vec<XgTask> = vec![];
	let mut tools:Vec<XgTool> = vec![];
	for e in parser.events() {
		match e {
				XmlEvent::StartElement {name, attributes, ..} => {
				match name.local_name.as_slice() {
						"Task" =>
						{
								tasks.push(xg_parse_task(&attributes));
						}
						"Tool" =>
						{
								tools.push(xg_parse_tool(&attributes));
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
}

fn xg_parse_task (attributes: &Vec<xml::attribute::OwnedAttribute>)->XgTask {
	let mut task = XgTask::new();
	for attr in attributes.iter() {
		match attr.name.local_name.as_slice()
			{
				"Name" =>
				{
						task.id = Some(attr.value.to_string());
				}
				"Caption" =>
				{
						task.title = Some(attr.value.to_string());
				}
				"Tool" =>
				{
						task.tool = Some(attr.value.to_string());
				}
				"WorkingDir" =>
				{
						task.working_dir = Some(attr.value.to_string());
				}
				"DependsOn" =>
				{
					for item in attr.value.split_str(";").collect::<Vec<&str>>().iter() {
							task.depends_on.push(item.to_string());
					}
				}
				_ =>
				{
				}
			}
	}
	task
}
fn xg_parse_tool (attributes: &Vec<xml::attribute::OwnedAttribute>)->XgTool {
	let mut tool = XgTool::new();
	for attr in attributes.iter() {
		match attr.name.local_name.as_slice()
			{
				"Name" =>
				{
						tool.id = Some(attr.value.to_string());
				}
				"Path" =>
				{
						tool.path = Some(attr.value.to_string());
				}
				"Params" =>
				{
						tool.params = Some(attr.value.to_string());
				}
				_ =>
				{
				}
			}
	}
	tool
}
