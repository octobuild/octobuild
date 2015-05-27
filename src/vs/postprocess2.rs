use std::hash::Hasher;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::io::{Read, Write, Error, ErrorKind};
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub enum PostprocessError {
	LiteralEol,
	LiteralEof,
	LiteralTooLong,
	EscapeEof,
	MarkerNotFound,
	InvalidLiteral,
	TokenTooLong,
}

const BUF_SIZE: usize = 0x10000;
				
impl Display for PostprocessError {
	fn fmt(&self, f: &mut Formatter) -> Result<(), ::std::fmt::Error> {
		match self {
			&PostprocessError::LiteralEol => write!(f, "unexpected end of line in literal"),
			&PostprocessError::LiteralEof => write!(f, "unexpected end of stream in literal"),
			&PostprocessError::LiteralTooLong => write!(f, "literal too long"),
			&PostprocessError::EscapeEof => write!(f, "unexpected end of escape sequence"),
			&PostprocessError::MarkerNotFound => write!(f, "can't find precompiled header marker in preprocessed file"),
			&PostprocessError::InvalidLiteral => write!(f, "can't create string from literal"),
			&PostprocessError::TokenTooLong => write!(f, "token too long"),
		}
	}
}

impl ::std::error::Error for PostprocessError {
	fn description(&self) -> &str {
		match self {
			&PostprocessError::LiteralEol => "unexpected end of line in literal",
			&PostprocessError::LiteralEof => "unexpected end of stream in literal",
			&PostprocessError::LiteralTooLong => "literal too long",
			&PostprocessError::EscapeEof => "unexpected end of escape sequence",
			&PostprocessError::MarkerNotFound => "can't find precompiled header marker in preprocessed file",
			&PostprocessError::InvalidLiteral => "can't create string from literal",
			&PostprocessError::TokenTooLong => "token too long",
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

pub fn filter_preprocessed(reader: &mut Read, writer: &mut Write, marker: &Option<String>, keep_headers: bool) -> Result<(), Error> {
	let mut state = ScannerState {
		buf_data: [0; BUF_SIZE],
		buf_read: 0,
		buf_copy: 0,
		buf_size: 0,

		reader: reader,
		writer: writer,

		keep_headers: keep_headers,
		marker: marker,

		utf8: false,
		header_found: false,
		entry_file: None,
		done: false,
	};
	try! (state.parse_bom());
	while try!(state.parse_line()) {
		if state.done {
			return state.copy_to_end();
		}
	}
	Err(Error::new(ErrorKind::InvalidInput, PostprocessError::MarkerNotFound))
}

struct ScannerState<'a> {
	buf_data: [u8; BUF_SIZE],
	buf_read: usize,
	buf_copy: usize,
	buf_size: usize,

	reader: &'a mut Read,
	writer: &'a mut Write,

	keep_headers: bool,
	marker: &'a Option<String>,
	
	utf8: bool,
	header_found: bool,
	entry_file: Option<String>,
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
		if self.buf_read == self.buf_size {
			try! (self.read());
		}
		if self.buf_size == 0 {
			return Ok(None)
		}
		Ok(Some(self.buf_data[self.buf_read]))
	}

	#[inline(always)]
	fn next(&mut self) {
		assert! (self.buf_read < self.buf_size);
		self.buf_read += 1;
	}

	#[inline(always)]
	fn read(&mut self) -> Result<usize, Error> {
		if self.buf_read == self.buf_size {
			try! (self.flush());
			self.buf_read = 0;
			self.buf_copy = 0;
			self.buf_size = try! (self.reader.read(&mut self.buf_data));
		}
		Ok(self.buf_size)
	}

	fn copy_to_end(&mut self) -> Result<(), Error> {
		try!(self.writer.write(&self.buf_data[self.buf_copy..self.buf_size]));
		self.buf_copy = 0;
		self.buf_size = 0;
		loop {
			match try! (self.reader.read(&mut self.buf_data)) {
				0 => {
					return Ok(());
				}
				size => {
					try! (self.writer.write(&self.buf_data[0..size]));
				}
			}
		}
	}


	fn flush(&mut self) -> Result<(), Error> {
		if self.buf_copy != self.buf_read {
			if self.keep_headers {
				try! (self.writer.write(&self.buf_data[self.buf_copy..self.buf_read]));
			}
			self.buf_copy = self.buf_read;
		}
		Ok(())
	}

	fn parse_bom(&mut self) -> Result<(), Error> {
		let bom: [u8; 3] = [0xEF, 0xBB, 0xBF];
		for bom_char in bom.iter() {
			match try! (self.peek()) {
				Some(c) if c == *bom_char => {
					self.next();
				}
				Some(_) => {return Ok(());},
				None => {return Ok(());},
			};
		}
		self.utf8 = true;
		Ok(())
	}

	fn parse_line(&mut self) -> Result<bool, Error> {
		try! (self.parse_spaces());
		match try!(self.peek()) {
			Some(b'#') => {
				self.next();
				self.parse_directive()
			}
			Some(_) => self.next_line(),
			None => Ok(false),
		}
	}

	fn next_line(&mut self) -> Result<bool, Error> {
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			for i in self.buf_read..self.buf_size {
				match self.buf_data[i] {
					b'\n' | b'\r' => {
						// end-of-line ::= newline | carriage-return | carriage-return newline
						self.buf_read = i + 1;
						return Ok(true);
					}
					_ => {
					}
				}
			}
			self.buf_read = self.buf_size;
			if try! (self.read()) == 0 {
				return Ok(false);
			}
		}
	}

	fn parse_directive(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces());
		match &try!(self.parse_token(0x20))[..] {
			b"line" => self.parse_directive_line(),
			b"pragma" => self.parse_directive_pragma(),
			_ => self.next_line(),
		}	
	}

	fn parse_directive_line(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces());
		let line = try!(self.parse_token(0x10));
		try!(self.parse_spaces());
		let (file, raw) = try!(self.parse_path(0x400));
		try!(self.next_line());
		self.entry_file = match self.entry_file.take() {
			Some(ref path) => {
				if self.header_found && (path == &file) {
					self.done = true;
					try! (self.write(b"#pragma hdrstop\n#line "));
					try! (self.write(&line));
					try! (self.write(b" "));
					try! (self.write(&raw));
					try! (self.write(b"\n"));
				}
				match self.marker {
					&Some(ref raw_path) => {
						let path = raw_path.replace("\\", "/");
						if (file == path) || Path::new(&file).ends_with(&Path::new(&path)) {
							self.header_found = true;
						}
					}
					&None => {}
				}
				Some(path.clone())
			}
			None => Some(file)
		};
		Ok(true)
	}

	fn parse_directive_pragma(&mut self) -> Result<bool, Error> {
		try!(self.parse_spaces());
		match &try!(self.parse_token(0x20))[..] {
			b"hdrstop" => {
				if !self.keep_headers {
					try! (self.write(b"#pragma hdrstop"));
				}
				self.done = true;
				Ok(true)
			},
			_ => {
				self.next_line()
			}
		}
	}

	fn parse_escape(&mut self) -> Result<u8, Error> {
		self.next();
		match try! (self.peek()) {
			Some(c) => {
				self.next();
				match c {
					b'n' => Ok(b'\n'),
					b'r' => Ok(b'\r'),
					b't' => Ok(b'\t'),
					c => Ok(c)
				}
			}
			None => {
				Err(Error::new(ErrorKind::InvalidInput, PostprocessError::EscapeEof))
			}
		}
	}

	fn parse_spaces(&mut self) -> Result<(), Error> {
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				match self.buf_data[self.buf_read] {
					// non-nl-white-space ::= a blank, tab, or formfeed character
					b' ' | b'\t' | b'\x0C' => {
						self.next();
					}
					_ => {
						return Ok(());
					}
				}
			}
			if try! (self.read()) == 0 {
				return Ok(());
			}
		}
	}

	fn parse_token(&mut self, limit: usize) -> Result<Vec<u8>, Error> {
		let mut token: Vec<u8> = Vec::with_capacity(limit);
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				let c: u8 = self.buf_data[self.buf_read];
				match c {
					// end-of-line ::= newline | carriage-return | carriage-return newline
					b'a'...b'z' | b'A'...b'Z' | b'0'...b'9' | b'_' => {
						token.push(c);
					}
					_ => {
						return Ok(token);
					}
				}
				self.next();
			}
			if token.len() > BUF_SIZE {
				return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::TokenTooLong))
			}
			if try! (self.read()) == 0 {
				return Ok(token);
			}
		}
	}

	fn literal_to_string(&self, bytes: Vec<u8>) -> Result<String, Error> {
		match self.utf8 {
			true => String::from_utf8(bytes).map_err(|_| Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral)),
			false => local_bytes_to_string(bytes),
		}
	}

	fn parse_path(&mut self, limit: usize) -> Result<(String, Vec<u8>), Error> {
		let mut token: Vec<u8> = Vec::with_capacity(limit);
		let mut raw: Vec<u8> = Vec::with_capacity(limit);
		let quote = try! (self.peek()).unwrap();
		raw.push(quote);
		self.next();
		loop {
			assert! (self.buf_size <= self.buf_data.len());
			while self.buf_read != self.buf_size {
				let c: u8 = self.buf_data[self.buf_read];
				match c {
					// end-of-line ::= newline | carriage-return | carriage-return newline
					b'\n' | b'\r' => {
						return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralEol));
					}
					b'\\' => {
						raw.push(b'\\');
						raw.push(c);
						match try!(self.parse_escape()) {
							b'\\' => token.push(b'/'),
							v => token.push(v),
						}
					}
					c => {
						self.next();
						raw.push(c);
						if c == quote {
							return Ok((try!(self.literal_to_string(token)), raw));
						}
						token.push(c);
					}
				}
			}
			if raw.len() > BUF_SIZE {
				return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralTooLong))
			}
			if try! (self.read()) == 0 {
					return Err(Error::new(ErrorKind::InvalidInput, PostprocessError::LiteralEof));
			}
		}
	}
}

fn local_bytes_to_string(vec: Vec<u8>) -> Result<String, Error> {
	#[cfg(unix)]
	fn local_bytes_to_string_inner(vec: Vec<u8>) -> Result<String, Error> {
		match OsString::from_bytes(vec) {
			Some(s) => s.into_string().map_err(|_| Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral)),
			None => Err(Error::new(ErrorKind::InvalidInput, PostprocessError::InvalidLiteral)),
		}
	}

	#[cfg(windows)]
	fn local_bytes_to_string_inner(vec: Vec<u8>) -> Option<OsString> {
		extern crate winapi;
		extern crate kernel32;

		use std::ptr;

		const MB_COMPOSITE: winapi::DWORD = 0x00000002; // use composite chars
		const MB_ERR_INVALID_CHARS: winapi::DWORD = 0x00000008; // use composite chars

		// Empty string
		if vec.len() == 0 {
			return Ok(String::new());
		}
		unsafe {
			// Get length of UTF-16 string
			let len = kernel32::MultiByteToWideChar(winapi::CP_ACP, MB_COMPOSITE | MB_ERR_INVALID_CHARS, vec.as_ptr() as winapi::LPCSTR, vec.len() as i32, ptr::null_mut(), 0);
			if len <= 0 {
				return None;
			}
			// Convert ANSI to UTF-16
			let mut utf: Vec<u16> = Vec::with_capacity(len as usize);
			utf.set_len(len as usize);
			if kernel32::MultiByteToWideChar(winapi::CP_ACP, MB_COMPOSITE | MB_ERR_INVALID_CHARS, vec.as_ptr() as winapi::LPCSTR, vec.len() as i32, utf.as_mut_ptr(), len) <= 0 {
				return None;
			}
			String::from_utf16(&utf).map_err(|e| Error::new(ErrorKind::InvalidInput, e))
		}
	}

	local_bytes_to_string_inner(vec)
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
#line 2 "sample.cpp"
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
#line 2 "sample.cpp"
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
			let mut result = Vec::with_capacity(source.len());
			super::filter_preprocessed(&mut Cursor::new(source.clone()), &mut result, &marker, keep_headers).unwrap();
			result
		});
	}
	
	#[bench]
	fn bench_check_filter(b: &mut Bencher) {
		bench_filter(b, "tests/filter_preprocessed.i", Some("c:\\bozaro\\github\\octobuild\\test_cl\\sample.h".to_string()), false)
	}

	/**
	 * Test for checking converting ANSI to Unicode characters on Ms Windows.
	 * Since the test is dependent on the environment, checked only Latin characters.
	 */
	#[test]
	fn test_local_bytes_to_string() {
		let vec = Vec::from("test\0data");
		assert_eq!(super::local_bytes_to_string(vec.clone()).ok(), String::from_utf8(vec).ok());
	}
}
