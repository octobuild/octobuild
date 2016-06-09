use std::sync::Arc;
use super::compiler::CommandEnv;

#[derive(Debug)]
#[derive(Clone)]
pub struct BuildTask {
    pub title: String,
    pub exec: String,
    pub args: Vec<String>,
    pub working_dir: String,
    pub env: Arc<CommandEnv>,
}
