// Parsing command line arguments from singe line.
// See also: http://msdn.microsoft.com/en-us/library/17w5ykft.aspx
pub fn parse(cmd: &str) -> Vec<String> {
	let mut args: Vec<String> = vec![];
	let mut arg: String = "".to_string();
	let mut escape = false;
	let mut quote = false;
	let mut data = false;
	for c in cmd.chars() {
		match c {
			' ' | '\t' => {
				if escape {
					arg.push('\\');
					escape = false;
				}
				if quote {
					arg.push(c);
					data = true;
				} else if data {
					args.push(arg);
					arg = "".to_string();
					data = false;
				}
			}
			'\\' => {
				if escape {
					arg.push(c);
				}
				escape = !escape;
				data = true;
			}
			'"' => {
				if escape {
					arg.push(c);
					escape = false;
				} else {
					quote = !quote;
				}
				data = true;
			}
			_ => {
				if escape {
					arg.push('\\');
					escape = false;
				}
				arg.push(c);
				data = true;
			}
		}
	}
	if data {
		args.push(arg);
	}
	return args;
}

pub fn expand_arg<F: Fn(&str) -> Option<String>>(arg: &str, resolver: &F) -> String {
	let mut result = "".to_string();
	let mut suffix = arg;
	loop {
		match suffix.find_str("$(") {
			Some(begin) => {
				match suffix.slice_from(begin).find_str(")") {
					Some(end) => {
						let name = suffix.slice(begin+2, begin + end);
						match resolver(name) {
							Some(ref value) => {
								result = result + suffix.slice_to(begin) + value.as_slice();
							}
							None => {
								result = result + suffix.slice_to(begin + end + 1);
							}
						}
						suffix = suffix.slice_from(begin + end + 1);
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
	let mut result:Vec<String> = vec![];
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
	assert_eq!(parse("a\\\\\\\\b d\"e f\"g h"), ["a\\\\b", "de fg", "h"]);
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
