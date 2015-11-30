extern crate octobuild;

use octobuild::filter::comments::CommentsRemover;

use std::env;
use std::io;
use std::fs::File;
use std::iter::FromIterator;

fn main() {
	for arg in Vec::from_iter(env::args())[1..].iter() {
		let mut input = CommentsRemover::new(File::open(arg).unwrap());
		let mut output = File::create(arg.to_string() + "~").unwrap();
		io::copy(&mut input, &mut output).unwrap();
	}
}