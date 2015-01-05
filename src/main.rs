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

enum OptionalString {
Value(String),
Missing,
}

impl fmt::Show for OptionalString {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	match self {
			&OptionalString::Value(ref v) => {
			write!(f, "{}", v)
		}
			&OptionalString::Missing => {
			write!(f, "missing")
		}
		}
}
}

struct XgTask {
id: OptionalString,
title: OptionalString,
tool: OptionalString,
working_dir: OptionalString,
depends_on: Vec<String>,
}

impl fmt::Show for XgTask {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "id={}, title={}, tool={}, working_dir={}, depends_on={}", self .id, self .title, self .tool, self .working_dir, self .depends_on)
}
}

struct XgTool {
id: OptionalString,
path: OptionalString,
params: OptionalString,
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
				XmlEvent::StartElement {name, attributes, namespace} => {
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
				XmlEvent::EndElement{name} => {
			}
				_ => {
			}
			}
	}
}

fn xg_parse_task (attributes: &Vec<xml::attribute::OwnedAttribute>)->XgTask {
	let mut task = XgTask {
	id: OptionalString::Missing,
	title: OptionalString::Missing,
	tool: OptionalString::Missing,
	working_dir: OptionalString::Missing,
	depends_on: vec![],
	};
	for attr in attributes.iter() {
		match attr.name.local_name.as_slice()
			{
				"Name" =>
				{
						task.id = OptionalString::Value(attr.value.to_string());
				}
				"Caption" =>
				{
						task.title = OptionalString::Value(attr.value.to_string());
				}
				"Tool" =>
				{
						task.tool = OptionalString::Value(attr.value.to_string());
				}
				"WorkingDir" =>
				{
						task.working_dir = OptionalString::Value(attr.value.to_string());
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
	let mut tool = XgTool {
	id: OptionalString::Missing,
	path: OptionalString::Missing,
	params: OptionalString::Missing,
	};
	for attr in attributes.iter() {
		match attr.name.local_name.as_slice()
			{
				"Name" =>
				{
						tool.id = OptionalString::Value(attr.value.to_string());
				}
				"Path" =>
				{
						tool.path = OptionalString::Value(attr.value.to_string());
				}
				"Params" =>
				{
						tool.params = OptionalString::Value(attr.value.to_string());
				}
				_ =>
				{
				}
			}
	}
	tool
}
