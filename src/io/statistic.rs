use std::cmp::max;

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

	pub fn add_hit(&mut self, bytes: usize) {
		self.hit_count += 1;
		self.hit_bytes += bytes;
	}

	pub fn add_miss(&mut self, bytes: usize) {
		self.miss_count += 1;
		self.miss_bytes += bytes;
	}

	pub fn to_string(&self) -> String {
		let total_count = self.hit_count + self.miss_count;
		format!(
			"Cache statistic: hit {} of {} ({} %), read {}, write {}, total {}",
			self.hit_count,
			total_count,
			self.hit_count * 100 / max(total_count, 1),
			self.hit_bytes,
			self.miss_bytes,
			self.hit_bytes + self.miss_bytes,
		)
	}
}

unsafe impl Send for Statistic {}
unsafe impl Sync for Statistic {}
