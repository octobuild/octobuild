use std::slice::Iter;

#[derive(Show)]
enum Directive {
	// raw, file
	Line(Vec<u8>, String),
	// raw
	HdrStop(Vec<u8>),
	// raw
	Unknown(Vec<u8>)
}

pub fn filter_preprocessed(input: &[u8], marker: &Option<String>, keep_headers: bool) -> Result<Vec<u8>, String> {
	let mut result: Vec<u8> = Vec::new();
	let mut line_begin = true;
	let mut iter = input.iter();
	// Entry file.
	let mut entry_file: Option<String> = None;
	let mut header_found: bool = false;
	loop {
		match iter.next() {
			Some(c) => {
				match *c {
					b'\n' | b'\r' => {
						if keep_headers {
							result.push(*c);
						}
						line_begin = true;
					}
					b'\t' | b' ' => {
						if keep_headers {
							result.push(*c);
						}
					}
					b'#' if line_begin => {
						let directive = read_directive(&mut iter);
						match directive {
							Directive::Line(raw, raw_file) => {
								let file = raw_file.replace("\\", "/");
								entry_file = match entry_file {
									Some(path) => {
										if header_found && (path  == file) {
											result.push_all(b"#pragma hdrstop\n");
											result.push(*c);
											result.push_all(raw.as_slice());
											break;
										}
										match *marker {
											Some(ref raw_path) => {
												let path = raw_path.replace("\\", "/");
												if file == path || Path::new(file.as_slice()).ends_with_path(&Path::new(path.as_slice())) {
													header_found = true;
												}
											}
											None => {}
										}
										Some(path)
									}
									None => Some(file)
								};
								if keep_headers {
									result.push(*c);
									result.push_all(raw.as_slice());
								}
							}
							Directive::HdrStop(raw) => {
								result.push(*c);
								result.push_all(raw.as_slice());
								break;
							}
							Directive::Unknown(raw) => {
								if keep_headers {
									result.push(*c);
									result.push_all(raw.as_slice());
								}
							}
						}
					}
					_ => {
						if keep_headers {
							result.push(*c);
						}
						line_begin = false;
					}
				}
			}
			None => {
				break;
			}
		}
	}
	loop {
		match iter.next() {
			Some(c) => {
				result.push(c.clone());
			}
			_ => {
				break;
			}
		}
	}
	match marker {
		&Some(ref path) if !header_found => {
			return Err(format!("Can't find marker in preprocessed file: {}", path));
		}
		_ => Ok(result)
	}
}

fn read_directive(iter: &mut Iter<u8>) -> Directive {
	let mut unknown: Vec<u8> = Vec::new();
	let (next, token) = read_token(None, iter, &mut unknown);
	match token.as_slice() {
		b"line" => read_directive_line(next, iter, unknown),
		b"pragma" => read_directive_pragma(next, iter, unknown),
		_ => {
			skip_line(next, iter, &mut unknown);
			Directive::Unknown(unknown)
		}
	}
}

fn read_token(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> (Option<u8>, Vec<u8>) {
	match skip_spaces(first, iter, unknown) {
		Some(first_char) => {
			let mut token: Vec<u8> = Vec::new();
			let mut escape = false;
			let quote: bool;
			if first_char == b'"' {
				quote = true;
			} else {
				token.push(first_char);
				quote = false;
			}
			loop {
				match iter.next() {
					Some(c) if quote => {
						unknown.push(*c);
						if escape {
							match *c {
								b'n' => token.push(b'\n'),
								b'r' => token.push(b'\r'),
								b't' => token.push(b'\t'),
								v => token.push(v)
							}
							escape = false;
						} else if *c == ('\\' as u8) {
							escape = true;
						} else if *c == b'"' {
							return (match iter.next() {
								Some(n) => {
									unknown.push(*n);
									Some(*n)
								}
								None => None
							}, token);
						} else {
							token.push(*c);
						}
					}
					Some(c) => {
						unknown.push(*c);
						match *c {
							b'a' ... b'z' | b'A' ... b'Z' | b'0' ... b'9' => {
								token.push(*c);
							}
							_ => {
								return (Some(*c), token);
							}
						}
					}
					None => {
						return (None, token);
					}
				}
			}
		}
		None => {
			return (None, Vec::new());
		}
	}
}

fn read_directive_line(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	// Line number
	let (next1, _) = read_token(first, iter, &mut unknown);
	// File name
	let (next2, file) = read_token(next1, iter, &mut unknown);
	skip_line(next2, iter, &mut unknown);
	Directive::Line(unknown, String::from_utf8_lossy(file.as_slice()).to_string())
}

fn read_directive_pragma(first: Option<u8>, iter: &mut Iter<u8>, mut unknown: Vec<u8>) -> Directive {
	let (next, token) = read_token(first, iter, &mut unknown);
	skip_line(next, iter, &mut unknown);
	match token.as_slice() {
		b"hdrstop" => Directive::HdrStop(unknown),
		_ => Directive::Unknown(unknown)
	}
}

fn skip_spaces(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) -> Option<u8> {
	match first {
		Some(c) => {
			match c {
				b'\n' | b'\r' => {return None;}
				b'\t' | b' ' => {}
				_ => {return first;}
			}
		}
		_ => {}
	}
	loop {
		match iter.next() {
			Some(c) => {
				unknown.push(*c);
				match c {
					&b'\n' | &b'\r' => {return None;}
					&b'\t' | &b' ' => {}
					_ => {return Some(*c);}
				}
			}
			None => {
				return None;
			}
		}
	}
}

fn skip_line(first: Option<u8>, iter: &mut Iter<u8>, unknown: &mut Vec<u8>) {
	match first {
		Some(c) => {
			match c {
				b'\n' | b'\r' => {return;}
				_ => {}
			}
		}
		_ => {}
	}
	loop {
		match iter.next() {
			Some(c) => {
				unknown.push(*c);
				match c {
					&b'\n' | &b'\r' => {return;}
					_ => {}
				}
			}
			None => {return;}
		}
	}
}

#[test]
fn test_filter_precompiled_keep() {
	let filtered = filter_preprocessed(br#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &Some("sample header.h".to_string()), true);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}

#[test]
fn test_filter_precompiled_remove() {
	let filtered = filter_preprocessed(br#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello1();
void hello2();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &Some("sample header.h".to_string()), false);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}

#[test]
fn test_filter_precompiled_hdrstop() {
	let filtered = filter_preprocessed(br#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &None, false);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}

#[test]
fn test_filter_precompiled_xxx() {
	let filtered = filter_preprocessed(br#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, &Some("e:\\work\\octobuild\\test_cl\\sample header.h".to_string()), true);
	assert_eq!(String::from_utf8_lossy(filtered.unwrap().as_slice()), r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#);
}
