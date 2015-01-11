#[derive(Show)]
#[derive(Clone)]
pub struct BuildTask {
	pub title: String,
	pub exec: String,
	pub args: Vec<String>,
	pub working_dir: String,
}
