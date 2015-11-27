use std::io::{Read, Error};

#[derive(Copy, Clone, Eq, PartialEq)]
enum MultiLineMark {
	None,
	Space,
	NewLine,
	Cr,
	Lf,
}

enum State {
	Code(MultiLineMark),
	Quote(u8),
	Escape(u8),
	SingleLineComment(MultiLineMark),
	MultLineComment(MultiLineMark),
}

/**
 * Filter for removing comments from preprocessed C/C++ code.
 */
pub struct CommentsRemover<R> {
	r: R,
	state: State,
	last: Option<u8>,
}

impl<R: Read> CommentsRemover<R> {
	pub fn new(r: R) -> CommentsRemover<R> {
		CommentsRemover {
			r: r,
			state: State::Code(MultiLineMark::None),
			last: None,
		}
	}
}

impl<R: Read> Read for CommentsRemover<R> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
		if buf.len() == 0 {
			return Ok(0);
		}
		let size = try! (self.r.read(buf));
		let mut offset = 0;
		for index in 0..size {	
			let c = buf[index];
			match self.state {
				State::Code(multiline) => {
					match c {
						b'"' | b'\'' => {
							self.state = State::Quote(c);
						}
						b'/' if self.last == Some(b'/') => {
							self.last = None;
							self.state = State::SingleLineComment(multiline);
						}
						b'*' if self.last == Some(b'/') => {
							self.last = None;
							self.state = State::MultLineComment(multiline);
						}
						c => {
							match self.last {
								Some(last) => {
									buf[offset] = last;
									offset += 1;
								}
								None => {
								}
							}
							self.last = Some(c);
							self.state = State::Code(match c {
								b'/' => multiline,
								b'\\' => MultiLineMark::Space,
								b'\t' | b' ' if multiline == MultiLineMark::Space => MultiLineMark::Space,
								b'\t' | b' ' => MultiLineMark::NewLine,
								b'\n' if multiline == MultiLineMark::Space => MultiLineMark::Lf,
								b'\n' if multiline == MultiLineMark::Cr => MultiLineMark::Cr,
								b'\r' if multiline == MultiLineMark::Space => MultiLineMark::Cr,
								b'\r' if multiline == MultiLineMark::Lf => MultiLineMark::Lf,
								_ => MultiLineMark::None,
							});
							continue;
						}
					};
				}
				State::Escape(q) => {
					self.state = State::Quote(q);
				}
				State::Quote(q) => {
					match c {
						b'\\' => {
							self.state = State::Escape(q);
						}
						c if c == q => {
							self.state = State::Code(MultiLineMark::None);
						}
						_ => {
						}
					}
				}
				State::SingleLineComment(multiline) => {
					match c {
						b'\n' | b'\r' => {
							self.state = State::Code(multiline);
						}
						_ => {
						}
					}
				}
				State::MultLineComment(multiline) => {
					match c {
						b'/' if self.last == Some(b'*') => {
							self.last = None;
							self.state = State::Code(multiline);
							continue;
						}
						v => {
							self.last = Some(v);
						}
					}
				}
			}
			match self.state {
				State::Code(_) | State::Quote(_) | State::Escape(_) => {
					match self.last {
						Some(c) => {
							buf[offset] = c;
							offset += 1;
						}
						None => {
						}
					}
					self.last = Some(c);
				}
				State::MultLineComment(multiline) => {
					match c {
						b'\n' | b'\r' => {
							match multiline {
								MultiLineMark::NewLine => {
									self.state = State::MultLineComment(match c {
										b'\n' => MultiLineMark::Lf,
										b'\r' => MultiLineMark::Cr,
										_ => MultiLineMark::None,
									});
									buf[offset] = b'\\';
									offset += 1;
								}
								MultiLineMark::Space => {
									self.state = State::MultLineComment(match c {
										b'\n' => MultiLineMark::Lf,
										b'\r' => MultiLineMark::Cr,
										_ => MultiLineMark::None,
									});
								}
								MultiLineMark::Lf if c == b'\n' => {
									buf[offset] = b'\\';
									offset += 1;
								}
								MultiLineMark::Cr if c == b'\r' => {
									buf[offset] = b'\\';
									offset += 1;
								}
							    _ => {							    	
							    }
							}
							buf[offset] = c;
							offset += 1;
						}
						_ => {
						}
					}
				}
				State::SingleLineComment(_) => {
				}
			}
		}
		if offset < buf.len() {
			match self.state {
				State::Code(_) | State::Quote(_) | State::Escape(_) => {
					match self.last {
						Some(c) if c == b'/' => {
						}
						Some(c) => {
							buf[offset] = c;
							offset += 1;
							self.last = None;
						}
						None => {
						}
					}
				}
				State::SingleLineComment(_) | State::MultLineComment(_) => {
				}
			}
		}
		if size == 0 {
			return Ok(offset);
		}
		match offset {
			0 => self.read(buf),
			_ => Ok(offset),
		}
	}
}

#[cfg(test)]
mod test {
	use std::io::{Read, Write, Cursor};
	use super::CommentsRemover;

	fn check_filter_pass(original: &str, expected: &str, block_size: usize) {
		let mut stream: Vec<u8> = Vec::new();
		stream.write(original.as_bytes()).unwrap();

		let mut filter = CommentsRemover::new(Cursor::new(stream));
		let mut actual = Vec::new();
		let mut buffer = vec![0; block_size];
		loop {
			let size = filter.read(&mut buffer).unwrap();
			if size == 0 {
				break;
			}
			actual.write(&buffer[0..size]).unwrap();
			assert!(actual.len() <= expected.len());
		}
		assert_eq!(expected, String::from_utf8(actual).unwrap());
	}

	fn check_filter(original: &str, expected: &str) {
		check_filter_pass(original, expected, expected.len());
		check_filter_pass(original, expected, original.len());
		check_filter_pass(original, expected, 1);
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
