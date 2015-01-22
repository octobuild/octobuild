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
