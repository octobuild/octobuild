extern crate xml;

use std::io::{File, BufferedReader};

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

#[test]
fn sample() {
	let file = File::open(&Path::new("graph-parser.xml")).unwrap();
	let reader = BufferedReader::new(file);

	let mut parser = EventReader::new(reader);
	let mut depth = 0;
	for e in parser.events() {
		match e {
			StartElement { name, attributes, namespace} => {
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
