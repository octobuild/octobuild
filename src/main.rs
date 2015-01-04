extern crate xml;

use std::os;

use std::io::{File, BufferedReader};
use std::fmt;

use xml::reader::EventReader;
use xml::reader::events::XmlEvent;
use xml::reader::events::XmlEvent::{StartElement, EndElement, Error};

fn indent(size: uint) -> String {
	let mut result = String::with_capacity(size*4);
	for _ in range(0, size) {
			result.push_str("    ");
	}
	result
}

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

struct IbTask {
name: String,
dependsOn: Vec<String>
}

impl fmt::Show for IbTask {
fn fmt(& self, f: &mut fmt::Formatter) -> fmt::Result {
	write!(f, "name={}, dependsOn={}", self .name, self .dependsOn)
}
}

fn sample() {
	let file = File::open(&Path::new("tests/graph-parser.xml")).unwrap();
	let reader = BufferedReader::new(file);

	let mut parser = EventReader::new(reader);
	let mut depth = 0;
	for e in parser.events() {
		match e {
				StartElement { name, attributes, namespace} => {
				match name.local_name.as_slice() {
						"Task" =>
						{
							let mut task = IbTask {
							name: "".to_string(),
							dependsOn: vec![]
							};
							println!("{}/{} {}!", indent(depth), name, attributes);
							for attr in attributes.iter() {
								//let a : int = attr.value;
								match attr.name.local_name.as_slice()
									{
										"Name" =>
										{
												task.name = attr.value.to_string();
										}
										"DependsOn" =>
										{
											for item in attr.value.split_str(";").collect::<Vec<&str>>().iter() {
												println!(" deps: {}", item);
												task.dependsOn.push(item.to_string());
											}
										}
										_ =>
										{}
									}
								println!(" attr: {}", attr);
								println!(" task: {}", task);
							}
						}
						_ => {}
					}
				println!("{}/{}", indent(depth), name);
				depth += 1;
			}
				EndElement{name} => {
				depth -= 1;
				println!("{}/{}", indent(depth), name);
			}
				_ => {
			}
			}
	}
}
