use std::hash::Hasher;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write, Error, ErrorKind};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub enum ScannerError {
	InvalidStream,
	IncludeEof,
	IncludeUnexpected(u8),
	TokenEof,
	TokenEol,
	LiteralEof,
	LiteralEol,
}

const BUF_SIZE: usize = 0x10000;
				
impl Display for ScannerError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&ScannerError::InvalidStream => write!(f, "internal error on stream reading"),
			&ScannerError::IncludeEof => write!(f, "can't parse #include directive: unexpected end of file"),
			&ScannerError::IncludeUnexpected(c) => write!(f, "can't parse #include directive: unexpected characted `{}`", c),
			&ScannerError::TokenEof => write!(f, "unexpected end of stream in token"),
			&ScannerError::TokenEol => write!(f, "unexpected end of line in token"),
			&ScannerError::LiteralEof => write!(f, "unexpected end of stream in literal"),
			&ScannerError::LiteralEol => write!(f, "unexpected end of line in literal"),
		}
	}
}

impl ::std::error::Error for ScannerError {
	fn description(&self) -> &str {
		match self {
			&ScannerError::InvalidStream => "internal error on stream reading",
			&ScannerError::IncludeEof => "can't parse #include directive: unexpected end of file",
			&ScannerError::IncludeUnexpected(_) => "can't parse #include directive: unexpected characted",
			&ScannerError::TokenEof => "unexpected end of stream in token",
			&ScannerError::TokenEol => "unexpected end of line in token",
			&ScannerError::LiteralEof => "unexpected end of stream in literal",
			&ScannerError::LiteralEol => "unexpected end of line in literal",
		}
	}

	fn cause(&self) -> Option<&::std::error::Error> {
		None
	}
}

#[derive(PartialEq)]
#[derive(Hash)]
#[derive(Eq)]
#[derive(Clone)]
#[derive(Debug)]
pub enum Include<T> {
	Quoted(T),
	Angle(T),
}

pub fn filter_preprocessed(base: &Option<PathBuf>, reader: &mut Read, writer: &mut Write, marker: &Option<String>, keep_headers: bool) -> Result<Vec<PathBuf>, Error> {
	try!(parse_source(reader, writer, keep_headers));
	Ok(Vec::new())
}

pub fn parse_source(reader: &mut Read, writer: &mut Write, keep_headers: bool) -> Result<(), Error> {
	let mut state = ScannerState {
		buf_data: [0; BUF_SIZE],
		buf_read: 0,
		buf_copy: 0,
		buf_size: 0,

		reader: reader,
		writer: writer,

		keep_headers: keep_headers,
		done: false,
	};
	try! (state.parse_bom());
	while try!(state.parse_line()) {
		if state.done {
			loop {
			 	match try!(state.peek()) {
			 		Some(v) => {try!(state.copy())}
			 		None => {return Ok(())}
			 	}
			}
		}
	}
	Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of stream"))
}

struct ScannerState<'a> {
	buf_data: [u8; BUF_SIZE],
	buf_read: usize,
	buf_copy: usize,
	buf_size: usize,

	reader: &'a mut Read,
	writer: &'a mut Write,

	keep_headers: bool,
	done: bool,
}

impl <'a> ScannerState<'a> {
	fn write(&mut self, data: &[u8]) -> Result<(), Error> {
		try! (self.flush());
		try! (self.writer.write(data));
		Ok(())
	}

	#[inline(always)]
	fn peek(&mut self) -> Result<Option<u8>, Error> {
		try! (self.read());
		if self.buf_size == 0 {
			return Ok(None)
		}
		Ok(Some(self.buf_data[self.buf_read]))
	}

	#[inline(always)]
	fn next(&mut self) -> Result<(), Error> {
		match self.keep_headers {
			true => self.copy(),
			false => self.skip(),
		}
	}

	#[inline(always)]
	fn skip(&mut self) -> Result<(), Error> {
		try! (self.read());
		if self.buf_read < self.buf_size {
			try!(self.flush());
			self.buf_read += 1;
			self.buf_copy = self.buf_read;
		}
		Ok(())
	}

	#[inline(always)]
	fn copy(&mut self) -> Result<(), Error> {
		try! (self.read());
		if self.buf_read < self.buf_size {
			self.buf_read += 1;
		}
		Ok(())
	}

	#[inline(always)]
	fn read(&mut self) -> Result<(), Error> {
		if self.buf_read == self.buf_size {
			try! (self.flush());
			self.buf_read = 0;
			self.buf_copy = 0;
			self.buf_size = try! (self.reader.read(&mut self.buf_data));
		}
		Ok(())
	}

	fn flush(&mut self) -> Result<(), Error> {
		if self.buf_copy != self.buf_read {
			try! (self.writer.write(&self.buf_data[self.buf_copy..self.buf_read]));
			self.buf_copy = self.buf_read;
		}
		Ok(())
	}

	fn parse_bom(&mut self) -> Result<(), Error> {
		let bom: [u8; 3] = [0xEF, 0xBB, 0xBF];
		for bom_char in bom.iter() {
			match try! (self.peek()) {
				Some(c) if c == *bom_char => {
					try!(self.next());
				}
				Some(_) => {return Ok(());},
				None => {return Ok(());},
			};
		}
		//try! (self.visitor.visit_bom());
		Ok(())
	}

	fn parse_line(&mut self) -> Result<bool, Error> {
		match try! (self.parse_spaces(true)) {
			Some(b'#') => {
				try!(self.next());
				self.parse_directive()
			}
			Some(_) => self.next_line(),
			None => Ok(false),
		}
	}

	fn next_line(&mut self) -> Result<bool, Error> {
		loop {
			match try! (self.peek()) {
				// end-of-line ::= newline | carriage-return | carriage-return newline
				Some(b'\n') | Some(b'\r') => {
					try! (self.next());
					return Ok(true);
				}
				Some(_) => {
					try! (self.next());
				}
				None => {
					return Ok(false);
				}
			}
		}
	}

	fn parse_directive(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces(false));
		match &try!(self.parse_token(0x20))[..] {
			b"line" => self.parse_directive_line(),
			b"pragma" => self.parse_directive_pragma(),
			token => {
				if self.keep_headers {
					try! (self.write(&token));
				}
				self.next_line()
			}
		}	
	}

	fn parse_directive_line(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces(false));
		let line = try!(self.parse_token(0x10));
		try!(self.parse_spaces(false));
		let file = try!(self.parse_literal(0x10000));
		println!("#LINE [{:?}] [{:?}]", String::from_utf8_lossy(&line), String::from_utf8_lossy(&file));
		self.next_line()
	}

	fn parse_directive_pragma(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces(false));
		match &try!(self.parse_token(0x20))[..] {
			b"hdrstop" => {
				if !self.keep_headers {
					try! (self.write(b"#pragma hdrstop"));
				}
				self.done = true;
				Ok(true)
			},
			token => {
				try! (self.write(&token));
				self.next_line()
			}
		}
	}

	fn parse_escape(&mut self) -> Result<u8, Error> {
		try! (self.next());
		match try! (self.peek()) {
			Some(c) => {
				try! (self.next());
				match c {
					b'n' => Ok(b'\n'),
					b'r' => Ok(b'\r'),
					b't' => Ok(b'\t'),
					c => Ok(c)
				}
			}
			None => {
				Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of stream"))
			}
		}
	}

	fn parse_spaces(&mut self, with_new_line: bool) -> Result<Option<u8>, Error> {
		loop {
			match try!(self.peek()) {
				Some(c) => {
					match c {
						// non-nl-white-space ::= a blank, tab, or formfeed character
						b' ' | b'\t' | b'\x0C' => {
							try!(self.next());
						}
						// end-of-line ::= newline | carriage-return | carriage-return newline
						b'\n' | b'\r' if with_new_line => {
							try!(self.next());
						}
						_ => {
							return Ok(Some(c));
						}
					}
				}
				None => {
					return Ok(None);
				}
			};
		}
	}

	fn parse_process_line(&mut self, copy: bool) -> Result<bool, Error> {
		loop {
			match try! (self.peek()) {
				// end-of-line ::= newline | carriage-return | carriage-return newline
				Some(b'\n') | Some(b'\r') => {
					return Ok(true);
				}
				Some(b'\\') => {
					try!(self.parse_escape());
				}
				Some(_) => {
					try!(match copy {
						true => self.copy(),
						false => self.skip(),
					});
				}
				None => {
					return Ok(false);
				}
			};
		}
	}

	fn parse_skip_multiline_comment(&mut self) -> Result<bool, Error> {
		self.flush();
		let mut asterisk = false;
		loop {
			while self.buf_read != self.buf_size {
				asterisk = match self.buf_data[self.buf_read] {
					b'*' => {
						self.buf_read += 1;
						true
					}
					b'/' if asterisk => {
						self.buf_read += 1;
						self.buf_copy = self.buf_read;
						return Ok(true);
					}
					// end-of-line ::= newline | carriage-return | carriage-return newline
					b'\n' | b'\r' => {
						try! (self.writer.write(&[self.buf_data[self.buf_read]]));
						self.buf_read += 1;
						false
					}
					_ => {
						self.buf_read += 1;
						false
					}
				}
			}
			try! (self.flush());
			self.buf_read = 0;
			self.buf_copy = 0;
			self.buf_size = try! (self.reader.read(&mut self.buf_data));
			if self.buf_size == 0 {
				return Ok(false);
			}
		}
	}

	fn parse_token(&mut self, limit: usize) -> Result<Vec<u8>, Error> {
		let mut token: Vec<u8> = Vec::with_capacity(limit);
		while token.len() < limit {
			match try! (self.peek()) {
				Some(c) => {
					match c {
						// end-of-line ::= newline | carriage-return | carriage-return newline
						b'a'...b'z' | b'A'...b'Z' | b'0'...b'9' | b'_' => {
							token.push(c);
						}
						_ => {
							break;
						}
					}
				}
				None => {
					break;
				}
			};
			try! (self.skip());
		}
		Ok(token)
	}

	fn parse_literal(&mut self, limit: usize) -> Result<Vec<u8>, Error> {
		let mut token: Vec<u8> = Vec::with_capacity(limit);
		let quote = try! (self.peek()).unwrap();
		try!(self.next());
		loop {
			match try!(self.peek()) {
				// end-of-line ::= newline | carriage-return | carriage-return newline
				Some(b'\n') | Some(b'\r') => {
					return Err(Error::new(ErrorKind::InvalidInput, ScannerError::LiteralEol));
				}
				Some(b'\\') => {
					let c = try!(self.parse_escape());
					token.push(c);
				}
				Some(c) => {
					try!(self.next());
					if c == quote {
						return Ok(token);
					}
					token.push(c);
				}
				None => {
					return Err(Error::new(ErrorKind::InvalidInput, ScannerError::LiteralEof));
				}
			}
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
		match super::filter_preprocessed(&None, &mut Cursor::new(stream), &mut writer, &marker, keep_headers) {
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
r#"#pragma hdrstop
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
			super::filter_preprocessed(&None, &mut Cursor::new(source.clone()), &mut result, &marker, keep_headers).unwrap();
			result
		});
	}
	
	#[bench]
	fn bench_check_filter(b: &mut Bencher) {
		bench_filter(b, "tests/filter_preprocessed.i", Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string()), false)
	}
}
