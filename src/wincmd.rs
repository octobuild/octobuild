// Parsing command line arguments from singe line.
// See also: http://msdn.microsoft.com/en-us/library/17w5ykft.aspx
pub fn parse(cmd: &str) -> Vec<String> {
	let mut args: Vec<String> = Vec::new();
	let mut arg: String = String::new();
	let mut slash: usize = 0;
	let mut quote = false;
	let mut data = false;
	for c in cmd.chars() {
		match c {
			' ' | '\t' => {
				arg = add_slashes(arg, if quote && ((slash % 2) == 0) {slash / 2} else {slash});
				slash = 0;
				if quote {
					arg.push(c);
					data = true;
				} else if data {
					args.push(arg);
					arg = String::new();
					data = false;
				}
			}
			'\\' => {
				slash = slash + 1;
				data = true;
			}
			'"' => {
				arg = add_slashes(arg, slash / 2);
				if (slash & 2) == 0 {
					quote = !quote;
				} else {
					arg.push(c);
				}
				slash = 0;
				data = true;
			}
			_ => {
				arg = add_slashes(arg, if quote && ((slash % 2) == 0) {slash / 2} else {slash});
				slash = 0;
				arg.push(c);
				data = true;
			}
		}
	}
	arg = add_slashes(arg, if quote && ((slash % 2) == 0) {slash / 2} else {slash});
	if data {
		args.push(arg);
	}
	return args;
}

pub fn join(args: &Vec<String>) -> String {
	let mut result = String::new();
	for arg in args.iter() {
		if result.len() > 0 {
			result = result + " ";
		}
		result = result + escape(arg.as_slice()).as_slice();
	}
	result
}

pub fn escape(arg: &str) -> String {
	// todo: Доделать
	arg.to_string()
}

fn add_slashes(mut line: String, count: usize) -> String {
	for _ in range(0, count) {
		line.push('\\');
	}
	line
}

pub fn expand_arg<F: Fn(&str) -> Option<String>>(arg: &str, resolver: &F) -> String {
	let mut result = String::new();
	let mut suffix = arg;
	loop {
		match suffix.find("$(") {
			Some(begin) => {
				match suffix[begin..].find(")") {
					Some(end) => {
						let name = &suffix[begin + 2..begin + end];
						match resolver(name) {
							Some(ref value) => {
								result = result + &suffix[..begin] + value.as_slice();
							}
							None => {
								result = result + &suffix[..begin + end + 1];
							}
						}
						suffix = &suffix[begin + end + 1..];
					}
					None => {
						result = result+suffix;
						break;
					}
				}
			}
			None => {
				result = result+ suffix;
				break;
			}
		}
	}
	result
}

pub fn expand_args<F: Fn(&str) -> Option<String>>(args: &Vec<String>, resolver: &F) -> Vec<String> {
	let mut result:Vec<String> = Vec::new();
	for arg in args.iter() {
		result.push(expand_arg(arg.as_slice(), resolver));
	}
	result
}

#[test]
fn test_parse_vars() {
	assert_eq!(expand_arg("A$(test)$(inner)$(none)B", &|name:&str|->Option<String> {
		match name {
			"test" => {
				Some("foo".to_string())
			}
			"inner" => {
				Some("$(bar)".to_string())
			}
			"none" => {
				None
			}
			_ => {
				assert!(false, format!("Unexpected value: {}", name));
				None
			}
		}
	}), "Afoo$(bar)$(none)B");
}

#[test]
fn test_parse_1() {
	assert_eq!(parse("\"abc\" d e"), ["abc", "d", "e"]);
}

#[test]
fn test_parse_2() {
	assert_eq!(parse(" \"abc\" d e "), ["abc", "d", "e"]);
}

#[test]
fn test_parse_3() {
	assert_eq!(parse("\"\" \"abc\" d e \"\""), ["", "abc", "d", "e", ""]);
}

#[test]
fn test_parse_4() {
	assert_eq!(parse("a\\\\b d\"e f\"g h"), ["a\\\\b", "de fg", "h"]);
}

#[test]
fn test_parse_5() {
	assert_eq!(parse("a\\\\\\\"b c d"), ["a\\\"b", "c", "d"]);
}

#[test]
fn test_parse_6() {
	assert_eq!(parse("a\\\\\\\\\"b c\" d e"), ["a\\\\b c", "d", "e"]);
}

#[test]
fn test_parse_7() {
	assert_eq!(parse("C:\\Windows\\System32 d e"), ["C:\\Windows\\System32", "d", "e"]);
}

#[test]
fn test_parse_8() {
	assert_eq!(parse("/TEST\"C:\\Windows\\System32\" d e"), ["/TESTC:\\Windows\\System32", "d", "e"]);
}
