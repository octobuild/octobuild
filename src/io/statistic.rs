#[derive(Clone)]
pub struct Statistic {
	pub hit_count: usize,
	pub hit_bytes: usize,
	pub miss_count: usize,
	pub miss_bytes: usize,
}

impl Statistic {
	pub fn new() -> Statistic {
		Statistic {
			hit_count: 0,
			hit_bytes: 0,
			miss_count: 0,
			miss_bytes: 0,
		}
	}
}