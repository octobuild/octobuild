use std::sync::Arc;
use std::collections::HashMap;

#[derive(Debug)]
#[derive(Clone)]
pub struct BuildTask {
	pub title: String,
	pub exec: String,
	pub args: Vec<String>,
	pub working_dir: String,
	pub env: Arc<HashMap<String, String>>,
}
