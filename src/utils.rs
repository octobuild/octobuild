extern crate "sha1-hasher" as sha1;

pub fn filter<T, R, F:Fn(&T) -> Option<R>>(args: &Vec<T>, filter:F) -> Vec<R> {
	let mut result: Vec<R> = Vec::new();
	for arg in args.iter() {
		match filter(arg) {
			Some(v) => {
				result.push(v);
			}
			None => {}
		}
	}
	result
}

pub fn hash_sha1(data: &[u8]) -> String {
	use std::hash::Writer;

	let mut hash = sha1::Sha1::new();
	hash.write(data);
	hash.hexdigest()
}