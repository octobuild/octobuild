use std::io::{Read, Error, ErrorKind};

#[derive(Copy, Clone, Eq, PartialEq)]
enum MultiLineMark {
	None,
	Space,
	NewLine,
	Cr,
	Lf,
}

enum State {
	Code(MultiLineMark, Option<u8>),
	Quote(u8, Option<u8>),
	Escape(u8),
	SingleLineComment(MultiLineMark),
	MultLineComment(MultiLineMark, bool, Option<u8>),
}

/**
 * Filter for removing comments from preprocessed C/C++ code.
 */
pub struct CommentsRemover<R> {
	r: R,
	// Parser state
	state: State,
	// Buffer with size/position
	buffer: Vec<u8>,
	offset: usize,
	limit: usize,
}

impl<R: Read> CommentsRemover<R> {
	pub fn new(r: R) -> CommentsRemover<R> {
		CommentsRemover::new_with_buffer(r, 4096)
	}

	pub fn new_with_buffer(r: R, buffer_size: usize) -> CommentsRemover<R> {
		assert!(buffer_size > 0);
		CommentsRemover {
			r: r,
			state: State::Code(MultiLineMark::None, None),
			buffer: vec! [0; buffer_size],
			offset: 0,
			limit: 0,
		}
	}

	fn preload(&mut self) -> Result<bool, Error> {
		if self.offset == self.limit {
			self.offset = 0;
			self.limit = 0;
		}
		let buffer_size = self.buffer.len();
		if self.limit < buffer_size {
			let size = try! (self.r.read(&mut self.buffer[self.offset..buffer_size]));
			self.limit += size;
		}
		Ok(self.limit > 0)
	}

	fn flush(&mut self, buf: &mut [u8], offset: usize) -> usize {
		match self.state {
			State::Escape(_) | State::SingleLineComment(_) | State::Quote(_, None) | State::Code(_, _) | State::MultLineComment(_, _, None) => offset,
			State::MultLineComment(multiline, end, Some(c)) => {
				self.state = State::MultLineComment(multiline, end, None);
				buf[offset] = c;
			 	offset + 1
			}
			State::Quote(q, Some(c)) => {
				self.state = State::Quote(q, None);
				buf[offset] = c;
			 	offset + 1
			}
		}
	}
}

impl<R: Read> Read for CommentsRemover<R> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
		if buf.len() == 0 {
			return Ok(0);
		}
		let mut index = 0;
		while index < buf.len() {
			index = self.flush(buf, index);
			if index == buf.len() {
				break;
			}
			assert!(index < buf.len());
			if self.offset >= self.limit {
				if try! (self.preload()) {
					continue;
				}
				match self.state {
					State::Code(_, None) | State::SingleLineComment(_) => {
						break;
					}
					State::Code(_, Some(last)) => {
						buf[index] = last;
						index += 1;
						self.state = State::Code(MultiLineMark::None, None);
						break;
					}
					_ => {
						return Err(Error::new(ErrorKind::InvalidInput, "Unexpected end of data"));
					}
				}
			}
			let c = self.buffer[self.offset];
			self.offset += 1;
			self.state = match self.state {
				State::Code(multiline, last) => {
					match c {
						b'"' | b'\'' => {
							match last {
								Some(last) => {
									buf[index] = last;
									index += 1;
								}
								None => {
								}
							}
							State::Quote(c, Some(c))
						}
						b'/' if last == Some(b'/') => State::SingleLineComment(multiline),
						b'*' if last == Some(b'/') => State::MultLineComment(multiline, false, None),
						c => {
							match last {
								Some(last) => {
									buf[index] = last;
									index += 1;
								}
								None => {
								}
							}
							self.state = State::Code(match c {
								b'/' => multiline,
								b'\\' => MultiLineMark::Space,
								b'\t' | b' ' if multiline == MultiLineMark::Space => MultiLineMark::Space,
								b'\t' | b' ' if multiline == MultiLineMark::None => MultiLineMark::None,
								b'\t' | b' '  => MultiLineMark::NewLine,
								b'\n' if multiline == MultiLineMark::Space => MultiLineMark::Lf,
								b'\n' if multiline == MultiLineMark::Cr => MultiLineMark::Cr,
								b'\r' if multiline == MultiLineMark::Space => MultiLineMark::Cr,
								b'\r' if multiline == MultiLineMark::Lf => MultiLineMark::Lf,
								_ => MultiLineMark::None,
							}, Some(c));
							continue;
						}
					}
				}
				State::Escape(q) => {
					State::Quote(q, Some(c))
				}
				State::Quote(q, last) => {
					assert!(last.is_none());
					buf[index] = c;
					index += 1;
					match c {
						b'\\' => State::Escape(q),
						c if c == q => State::Code(MultiLineMark::None, None),
						_ => State::Quote(q, None),
					}
				}
				State::SingleLineComment(multiline) => {
					match c {
						b'\n' | b'\r' => State::Code(multiline, Some(c)),
						_ => State::SingleLineComment(multiline),
					}
				}
				State::MultLineComment(multiline, end, last) => {
					assert!(last.is_none());
					match c {
						b'/' if end => State::Code(multiline, None),
						b'*' => State::MultLineComment(multiline, true, None),
						b'\n' | b'\r' => {
							match multiline {
								MultiLineMark::NewLine => {
									buf[index] = b'\\';
									index += 1;
									State::MultLineComment(match c {
										b'\n' => MultiLineMark::Lf,
										b'\r' => MultiLineMark::Cr,
										_ => MultiLineMark::None,
									}, false, Some(c))
								}
								MultiLineMark::Space => {
									State::MultLineComment(match c {
										b'\n' => MultiLineMark::Lf,
										b'\r' => MultiLineMark::Cr,
										_ => MultiLineMark::None,
									}, false, Some(c))
								}
								MultiLineMark::Lf if c == b'\n' => {
									buf[index] = b'\\';
									index += 1;
									State::MultLineComment(multiline, false, Some(c))
								}
								MultiLineMark::Cr if c == b'\r' => {
									buf[index] = b'\\';
									index += 1;
									State::MultLineComment(multiline, false, Some(c))
								}
							    _ => {
							    	State::MultLineComment(multiline, false, Some(c))
							    }
							}
						}
						_ => State::MultLineComment(multiline, false, None),
					}
				}
			}
		}
		if index < buf.len() {
			index = self.flush(buf, index);
		}
		return Ok(index);
	}
}

#[cfg(test)]
mod test {
	use std::io::{Read, Write, Cursor};
	use super::CommentsRemover;

	fn check_filter_pass(original: &str, expected: &str, block_size: usize) {
		let mut stream: Vec<u8> = Vec::new();
		stream.write(original.as_bytes()).unwrap();

		let mut filter = CommentsRemover::new_with_buffer(Cursor::new(stream), block_size);
		let mut actual = Vec::new();
		let mut buffer = vec![0; block_size];
		loop {
			let size = filter.read(&mut buffer).unwrap();
			if size == 0 {
				break;
			}
			actual.write(&buffer[0..size]).unwrap();
			assert!(actual.len() <= expected.len() * 2);
		}
		assert_eq!(expected, String::from_utf8(actual).unwrap());
	}

	fn check_filter(original: &str, expected: &str) {
		check_filter_pass(original, expected, expected.len());
		check_filter_pass(original, expected, original.len());
		check_filter_pass(original, expected, 1);
		check_filter_pass(original, expected, 3);
		check_filter_pass(original, expected, 10);
		check_filter_pass(original, expected, expected.len() - 1);
		check_filter_pass(original, expected, original.len() - 1);
	}

	#[test]
	fn test_filter_no_comments() {
		let source = r#"#line 1 "sample.cpp"
#line 1 "e:/work/octobuild/test_cl/sample header.h"
# pragma once
void hello();
#line 2 "sample.cpp"

int main(int argc, char **argv) {
	return 0;
}
"#;
		check_filter(source, source);
	}

	#[test]
	fn test_filter_simple_comments() {
		check_filter(
r#"// Some data
#line 1 "e:/work//octobuild/test_cl/sample /* header */.h"
# pragma once /*/ foo */
void hello();
//#line 2 "sample.cpp"

int main(int argc, char **argv /* // Arguments */) {
/*
 * Multiline
 */
	return 0;
}
"#,
r#"
#line 1 "e:/work//octobuild/test_cl/sample /* header */.h"
# pragma once 
void hello();


int main(int argc, char **argv ) {



	return 0;
}
"#
		);
	}

	#[test]
	fn test_filter_define_comments() {
		check_filter(
r#"#define A "A1" \ 
   /* Foo
Bar */ \
          "A2"

#define B "B1" \  /* Buzz */ /*X*//* Foo
Bar */ \
          "B2"

int main() {
	/** Foo
	 */
    return 0;
}
"#,
r#"#define A "A1" \ 
   \
 \
          "A2"

#define B "B1" \   
 \
          "B2"

int main() {
	

    return 0;
}
"#
		);
	}
}
