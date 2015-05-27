use std::collections::HashSet;
use std::io::{Read, Write, Error};
use std::path::Path;

use super::super::utils::DEFAULT_BUF_SIZE;
use super::super::io::binary::*;

#[derive(Debug)]
enum Directive {
	// raw, file
	Line(Vec<u8>, String),
	// raw
	HdrStop(Vec<u8>),
	// raw
	Unknown(Vec<u8>)
}

pub fn filter_preprocessed(reader: &mut Read, writer: &mut Write, marker: &Option<String>, keep_headers: bool) -> Result<(), Error> {
	let mut line_begin = true;
	// Entry file.
	let mut entry_file: Option<String> = None;
	let mut header_found: bool = false;
	let mut header_files: HashSet<String> = HashSet::new();
	loop {
		let c = try! (read_u8(reader));
		match c {
			b'\n' | b'\r' => {
				if keep_headers {
					try! (write_u8(writer, c));
				}
				line_begin = true;
			}
			b'\t' | b' ' => {
				if keep_headers {
					try! (write_u8(writer, c));
				}
			}
			b'#' if line_begin => {
				let directive = try! (read_directive(c, reader));
				match directive {
					Directive::Line(raw, raw_file) => {
						let file = raw_file.replace("\\", "/");
						entry_file = match entry_file {
							Some(path) => {
								if header_found && (path == file) {
									try! (writer.write_all(b"#pragma hdrstop\n"));
									try! (writer.write_all(&raw));
									break;
								}
								match *marker {
									Some(ref raw_path) => {
										let path = raw_path.replace("\\", "/");
										if file == path || Path::new(&file).ends_with(&Path::new(&path)) {
											header_found = true;
										}
									}
									None => {}
								}
								Some(path)
							}
							None => Some(file.clone())
						};
						header_files.insert(file);
						if keep_headers {
							try! (writer.write_all(&raw));
						}
					}
					Directive::HdrStop(raw) => {
						try! (writer.write_all(&raw));
						break;
					}
					Directive::Unknown(raw) => {
						if keep_headers {
							try! (writer.write_all(&raw));
						}
					}
				}
			}
			_ => {
				if keep_headers {
					try! (write_u8(writer, c));
				}
				line_begin = false;
			}
		}
	}
	// Copy end of stream.
	let mut buf: [u8; DEFAULT_BUF_SIZE] = [0; DEFAULT_BUF_SIZE];
	loop {
		let size = try! (reader.read(&mut buf));
		if size <= 0 {
			break;
		}
		try! (writer.write_all(&buf[0..size]));
	}
	Ok(())
}

fn read_directive(first: u8, reader: &mut Read) -> Result<Directive, Error> {
	let mut raw: Vec<u8> = Vec::new();
	raw.push(first);
	let (next, token) = try! (read_token(None, reader, &mut raw));
	match &token[..] {
		b"line" => read_directive_line(next, reader, raw),
		b"pragma" => read_directive_pragma(next, reader, raw),
		_ => {
			try! (skip_line(next, reader, &mut raw));
			Ok(Directive::Unknown(raw))
		}
	}
}

fn read_token(first: Option<u8>, reader: &mut Read, raw: &mut Vec<u8>) -> Result<(Option<u8>, Vec<u8>), Error> {
	match try! (skip_spaces(first, reader, raw)) {
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
				let c = try! (read_u8(reader));
				raw.push(c);
				if quote {
					if escape {
						match c {
							b'n' => token.push(b'\n'),
							b'r' => token.push(b'\r'),
							b't' => token.push(b'\t'),
							v => token.push(v)
						}
						escape = false;
					} else if c == ('\\' as u8) {
						escape = true;
					} else if c == b'"' {
						let n = try! (read_u8(reader));
						raw.push(n);
						return Ok((Some(n), token));
					} else {
						token.push(c);
					}
				} else {
					match c {
						b'a' ... b'z' | b'A' ... b'Z' | b'0' ... b'9' => {
							token.push(c);
						}
						_ => {
							return Ok((Some(c), token));
						}
					}
				}
			}
		}
		None => {
			return Ok((None, Vec::new()));
		}
	}
}

fn read_directive_line(first: Option<u8>, reader: &mut Read, mut raw: Vec<u8>) -> Result<Directive, Error> {
	// Line number
	let (next1, _) = try! (read_token(first, reader, &mut raw));
	// File name
	let (next2, file) = try! (read_token(next1, reader, &mut raw));
	try! (skip_line(next2, reader, &mut raw));
	Ok(Directive::Line(raw, String::from_utf8_lossy(&file).to_string()))
}

fn read_directive_pragma(first: Option<u8>, reader: &mut Read, mut raw: Vec<u8>) -> Result<Directive, Error> {
	let (next, token) = try! (read_token(first, reader, &mut raw));
	try! (skip_line(next, reader, &mut raw));
	match &token[..] {
		b"hdrstop" => Ok(Directive::HdrStop(raw)),
		_ => Ok(Directive::Unknown(raw))
	}
}

fn skip_spaces(first: Option<u8>, reader: &mut Read, raw: &mut Vec<u8>) -> Result<Option<u8>, Error> {
	match first {
		Some(c) => {
			match c {
				b'\n' | b'\r' => {return Ok(None);}
				b'\t' | b' ' => {}
				_ => {return Ok(first);}
			}
		}
		_ => {}
	}
	loop {
		let c = try! (read_u8(reader));
		try! (write_u8(raw, c));
		match c {
			b'\n' | b'\r' => {return Ok(None);}
			b'\t' | b' ' => {}
			_ => {return Ok(Some(c));}
		}
	}
}

fn skip_line(first: Option<u8>, reader: &mut Read, raw: &mut Vec<u8>) -> Result<(), Error> {
	match first {
		Some(c) => {
			match c {
				b'\n' | b'\r' => {return Ok(());}
				_ => {}
			}
		}
		_ => {}
	}
	loop {
		let c = try! (read_u8(reader));
		try! (write_u8(raw, c));
		match c {
			b'\n' | b'\r' => {return Ok(());}
			_ => {}
		}
	}
}

#[cfg(test)]
mod test {
	extern crate test;

	use std::io::{Read, Write, Cursor};
	use std::fs::File;
	use self::test::Bencher;

	fn check_filter(original: &str, expected: &str, marker: Option<String>, keep_headers: bool) {
		let mut writer: Vec<u8> = Vec::new();
		let mut stream: Vec<u8> = Vec::new();
		stream.write(&original.as_bytes()[..]).unwrap();
		match super::filter_preprocessed(&mut Cursor::new(stream), &mut writer, &marker, keep_headers) {
			Ok(_) => {assert_eq! (String::from_utf8_lossy(&writer), expected)}
			Err(e) => {panic! (e);}
		}
	}

	#[test]
	fn test_filter_precompiled_keep() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("sample header.h".to_string()), true)
	}

	#[test]
	fn test_filter_precompiled_remove() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello1();
void hello2();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, 
r#"#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("sample header.h".to_string()), false);
	}

	#[test]
	fn test_filter_precompiled_hdrstop() {
		check_filter(
r#"#line 1 "sample.cpp"
 #line 1 "e:/work/octobuild/test_cl/sample header.h"
void hello();
# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"# pragma  hdrstop
void data();
# pragma once
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, None, false);
	}

	#[test]
	fn test_filter_precompiled_winpath() {
		check_filter(
r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#,
r#"#line 1 "sample.cpp"
#line 1 "e:\\work\\octobuild\\test_cl\\sample header.h"
# pragma once
void hello();
#pragma hdrstop
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#, Some("e:\\work\\octobuild\\test_cl\\sample header.h".to_string()), true);
	}

	fn bench_filter(b: &mut Bencher, path: &str, marker: Option<String>, keep_headers: bool) {
		let mut source = Vec::new();
		File::open(path).unwrap().read_to_end(&mut source).unwrap();
		b.iter(|| {
			let mut result = Vec::new();
			super::filter_preprocessed(&mut Cursor::new(source.clone()), &mut result, &marker, keep_headers).unwrap();
			result
		});
	}
	
	#[bench]
	fn bench_check_filter(b: &mut Bencher) {
		bench_filter(b, "tests/filter_preprocessed.i", Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string()), false)
	}
}
