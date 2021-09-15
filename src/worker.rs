use std::borrow::Cow;
use std::cmp::{max, min};
use std::io::{Error, ErrorKind};
use std::sync::Arc;

use petgraph::graph::NodeIndex;
use petgraph::{EdgeDirection, Graph};

use crate::compiler::{CommandInfo, CompilationTask, Compiler, OutputInfo, SharedState, Toolchain};

pub type BuildGraph = Graph<Arc<BuildTask>, ()>;

pub struct BuildTask {
    pub title: String,
    pub action: BuildAction,
}

pub enum BuildAction {
    Empty,
    Exec(CommandInfo, Vec<String>),
    Compilation(Arc<dyn Toolchain>, CompilationTask),
}

pub struct BuildResult<'a> {
    // Completed task
    pub task: &'a BuildTask,
    // Worker number
    pub worker: usize,
    // Build result
    pub result: &'a Result<OutputInfo, Error>,
    // Completed task count
    pub completed: usize,
    // Total task count
    pub total: usize,
}

struct ResultMessage {
    index: NodeIndex,
    task: Arc<BuildTask>,
    worker: usize,
    result: Result<OutputInfo, Error>,
}

struct TaskMessage {
    index: NodeIndex,
    task: Arc<BuildTask>,
}

impl<'a> BuildResult<'a> {
    fn new(message: &'a ResultMessage, completed: &mut usize, total: usize) -> Self {
        *completed += 1;
        BuildResult {
            worker: message.worker,
            task: &message.task,
            result: &message.result,
            completed: *completed,
            total,
        }
    }
}

impl BuildAction {
    pub fn create_tasks<C: Compiler>(
        compiler: &C,
        command: CommandInfo,
        args: &[String],
        title: &str,
    ) -> Vec<BuildAction> {
        let actions: Vec<BuildAction> = compiler
            .create_tasks(command.clone(), args)
            .map(|tasks| {
                tasks
                    .into_iter()
                    .map(|(toolchain, task)| BuildAction::Compilation(toolchain, task))
                    .collect()
            })
            .unwrap_or_else(|e| {
                println!("Can't use octobuild for task {}: {}", title, e);
                Vec::new()
            });
        if actions.is_empty() {
            return vec![BuildAction::Exec(command, args.to_vec())];
        }
        actions
    }

    pub fn title(&self) -> Cow<str> {
        match self {
            BuildAction::Empty => Cow::Borrowed(""),
            BuildAction::Exec(_, ref args) => Cow::Owned(format!("{:?}", args)),
            BuildAction::Compilation(_, ref task) => {
                Cow::Borrowed(task.input_source.to_str().unwrap_or(""))
            }
        }
    }
}

pub fn validate_graph<N, E>(graph: Graph<N, E>) -> Result<Graph<N, E>, Error> {
    let mut completed: Vec<bool> = Vec::with_capacity(graph.node_count());
    let mut queue: Vec<NodeIndex> = Vec::with_capacity(graph.node_count());
    if graph.node_count() == 0 {
        return Ok(graph);
    }
    for index in 0..graph.node_count() {
        completed.push(false);
        queue.push(NodeIndex::new(index));
    }
    let mut count: usize = 0;
    let mut i: usize = 0;
    while i < queue.len() {
        let index = queue[i];
        if (!completed[index.index()]) && (is_ready(&graph, &completed, index)) {
            completed[index.index()] = true;
            for neighbor in graph.neighbors_directed(index, EdgeDirection::Incoming) {
                queue.push(neighbor);
            }
            count += 1;
            if count == completed.len() {
                return Ok(graph);
            }
        }
        i += 1;
    }
    Err(Error::new(
        ErrorKind::InvalidInput,
        "Found cycles in build dependencies",
    ))
}

fn execute_until_failed<F>(
    graph: &BuildGraph,
    tx_task: crossbeam::channel::Sender<TaskMessage>,
    rx_result: &crossbeam::channel::Receiver<ResultMessage>,
    count: &mut usize,
    update_progress: F,
) -> Result<Option<i32>, Error>
where
    F: Fn(BuildResult) -> Result<(), Error>,
{
    let mut completed: Vec<bool> = vec![false; graph.node_count()];
    for index in graph.externals(EdgeDirection::Outgoing) {
        tx_task
            .send(TaskMessage {
                index,
                task: graph.node_weight(index).unwrap().clone(),
            })
            .map_err(|e| Error::new(ErrorKind::Other, e))?;
    }

    for message in rx_result.iter() {
        assert!(!completed[message.index.index()]);

        update_progress(BuildResult::new(&message, count, graph.node_count()))?;
        let result = message.result?;
        if !result.success() {
            let status = result.status;
            return Ok(status);
        }
        completed[message.index.index()] = true;

        for source in graph.neighbors_directed(message.index, EdgeDirection::Incoming) {
            if is_ready(graph, &completed, source) {
                tx_task
                    .send(TaskMessage {
                        index: source,
                        task: graph.node_weight(source).unwrap().clone(),
                    })
                    .map_err(|e| Error::new(ErrorKind::Other, e))?;
            }
        }

        if *count == completed.len() {
            return Ok(Some(0));
        }
    }
    panic!("Unexpected end of result pipe");
}

fn is_ready<N, E>(graph: &Graph<N, E>, completed: &[bool], source: NodeIndex) -> bool {
    for neighbor in graph.neighbors_directed(source, EdgeDirection::Outgoing) {
        if !completed[neighbor.index()] {
            return false;
        }
    }
    true
}

pub fn execute_graph<F>(
    state: &SharedState,
    build_graph: BuildGraph,
    process_limit: usize,
    update_progress: F,
) -> Result<Option<i32>, Error>
where
    F: Fn(BuildResult) -> Result<(), Error>,
{
    let graph = validate_graph(build_graph)?;
    if graph.node_count() == 0 {
        return Ok(Some(0));
    }

    if graph.node_count() == 1 {
        let task = &graph.raw_nodes()[0].weight;
        let result = execute_compiler(state, task);
        update_progress(BuildResult {
            worker: 0,
            completed: 1,
            total: 1,
            result: &result,
            task,
        })?;
        return result.map(|output| output.status);
    }

    let (tx_result, rx_result) = crossbeam::channel::unbounded::<ResultMessage>();
    let (tx_task, rx_task) = crossbeam::channel::unbounded::<TaskMessage>();
    let num_cpus = max(1, min(process_limit, graph.node_count()));
    crossbeam::scope(|scope| {
        for worker_id in 0..num_cpus {
            let local_rx_task = rx_task.clone();
            let local_tx_result = tx_result.clone();
            scope.spawn(move |_| {
                while let Ok(message) = local_rx_task.recv() {
                    match local_tx_result.send(ResultMessage {
                        index: message.index,
                        worker: worker_id,
                        result: execute_compiler(state, &message.task),
                        task: message.task,
                    }) {
                        Ok(_) => {}
                        Err(_) => {
                            break;
                        }
                    }
                }
            });
        }
        drop(tx_result);
        // Run all tasks.
        let mut count: usize = 0;
        let result =
            execute_until_failed(&graph, tx_task, &rx_result, &mut count, &update_progress);
        // Cleanup task queue.
        for _ in rx_task.try_iter() {}
        // Wait for in progress task completion.
        for message in rx_result.iter() {
            update_progress(BuildResult::new(&message, &mut count, graph.node_count()))?;
        }
        result
    })
    .unwrap()
}

fn execute_compiler(state: &SharedState, task: &BuildTask) -> Result<OutputInfo, Error> {
    match &task.action {
        BuildAction::Empty => Ok(OutputInfo {
            status: Some(0),
            stderr: Vec::new(),
            stdout: Vec::new(),
        }),
        BuildAction::Exec(ref command, ref args) => state.wrap_slow(|| {
            command
                .to_command()
                .args(args)
                .output()
                .map(OutputInfo::new)
        }),
        BuildAction::Compilation(ref toolchain, ref task) => {
            toolchain.compile_task(state, task.clone())
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use crate::compiler::SharedState;
    use crate::config::Config;

    use super::*;

    #[test]
    fn test_execute_graph_empty() {
        let state = SharedState::new(&Config::defaults().unwrap()).unwrap();
        let graph = BuildGraph::new();
        execute_graph(&state, graph, 2, |_| {
            unreachable!();
        })
        .unwrap();
    }

    #[test]
    fn test_execute_graph_single() {
        let state = SharedState::new(&Config::defaults().unwrap()).unwrap();

        // Simple two tasks graph
        let mut graph = BuildGraph::new();
        graph.add_node(Arc::new(BuildTask {
            title: "task 1".to_string(),
            action: BuildAction::Empty,
        }));

        let result = Mutex::new(Vec::new());
        execute_graph(&state, graph, 4, |r| {
            result.lock().unwrap().push(r.task.title.clone());
            Ok(())
        })
        .unwrap();

        let actual: Vec<String> = result.lock().unwrap().clone();
        assert_eq!(actual, vec!["task 1".to_string()]);
    }

    // Test for #19 issue (https://github.com/bozaro/octobuild/issues/19)
    #[test]
    fn test_execute_graph_no_hang() {
        let state = SharedState::new(&Config::defaults().unwrap()).unwrap();

        // Simple two tasks graph
        let mut graph = BuildGraph::new();
        let t1 = graph.add_node(Arc::new(BuildTask {
            title: "task 1".to_string(),
            action: BuildAction::Empty,
        }));
        let t2 = graph.add_node(Arc::new(BuildTask {
            title: "task 2".to_string(),
            action: BuildAction::Empty,
        }));
        graph.add_edge(t2, t1, ());

        let result = Mutex::new(Vec::new());
        execute_graph(&state, graph, 4, |r| {
            result.lock().unwrap().push(r.task.title.clone());
            Ok(())
        })
        .unwrap();

        let actual: Vec<String> = result.lock().unwrap().clone();
        assert_eq!(actual, vec!["task 1".to_string(), "task 2".to_string()]);
    }
}
