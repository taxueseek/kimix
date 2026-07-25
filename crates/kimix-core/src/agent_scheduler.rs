//! 任务调度器：DAG 并行调度 + Coordinator-Worker 分离。
//!
//! # 调度算法
//!
//! ```text
//! 1. 构建任务 DAG（依赖关系图）
//! 2. Kahn 拓扑排序得到执行层（每层内任务无依赖，可并行）
//! 3. 每层按 max_parallel 分块，块内并行 spawn workers
//! 4. 收集结果，传递给下一层；超 max_depth 则失败
//! ```

use std::collections::{HashMap, VecDeque};
use std::future::Future;

use futures::future::join_all;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::Direction;
use tracing::{debug, info, warn};

/// 任务 ID。
pub type TaskId = uuid::Uuid;

/// 任务类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskKind {
    /// 只读探索（Plan Mode 阶段 1）。
    Explore,
    /// 生成计划（Plan Mode 阶段 2）。
    Plan,
    /// 执行具体任务。
    Execute,
    /// 编排子任务。
    Coordinate,
}

/// 任务定义。
#[derive(Debug, Clone)]
pub struct Task {
    /// 唯一标识。
    pub id: TaskId,
    /// 任务类型。
    pub kind: TaskKind,
    /// 自然语言指令。
    pub instruction: String,
    /// 允许使用的工具名列表。
    pub tools: Vec<String>,
    /// 可选模型偏好（路由提示）。
    pub model_pref: Option<String>,
    /// 前置依赖任务 ID（必须先完成）。
    pub deps: Vec<TaskId>,
}

impl Task {
    /// 构造无依赖的简单任务。
    pub fn new(kind: TaskKind, instruction: impl Into<String>) -> Self {
        Self {
            id: TaskId::new_v4(),
            kind,
            instruction: instruction.into(),
            tools: Vec::new(),
            model_pref: None,
            deps: Vec::new(),
        }
    }

    /// 指定 ID 与依赖构造任务（测试与确定性场景用）。
    pub fn with_id(id: TaskId, kind: TaskKind, instruction: impl Into<String>, deps: Vec<TaskId>) -> Self {
        Self {
            id,
            kind,
            instruction: instruction.into(),
            tools: Vec::new(),
            model_pref: None,
            deps,
        }
    }
}

/// 单任务执行结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// 对应任务 ID。
    pub task_id: TaskId,
    /// 是否成功。
    pub success: bool,
    /// 质量分（约定 0.0–10.0，由执行器填写）。
    pub quality: f64,
    /// 延迟（毫秒）。
    pub latency_ms: u64,
    /// 成本（美元）。
    pub cost_usd: f64,
}

/// 任务图（DAG）。
#[derive(Debug)]
pub struct TaskGraph {
    graph: DiGraph<Task, ()>,
    node_map: HashMap<TaskId, NodeIndex>,
}

impl TaskGraph {
    /// 从任务列表构建 DAG。
    ///
    /// 未知依赖 ID 会被跳过并记 warn 日志（不中断构图）。
    pub fn from_tasks(tasks: Vec<Task>) -> Self {
        let mut graph = DiGraph::new();
        let mut node_map = HashMap::new();

        for task in &tasks {
            let idx = graph.add_node(task.clone());
            node_map.insert(task.id, idx);
        }

        for task in &tasks {
            let Some(&to) = node_map.get(&task.id) else {
                continue;
            };
            for dep in &task.deps {
                match node_map.get(dep) {
                    Some(&from) => {
                        graph.add_edge(from, to, ());
                    }
                    None => {
                        warn!(
                            task_id = %task.id,
                            missing_dep = %dep,
                            "skipping unknown task dependency"
                        );
                    }
                }
            }
        }

        Self { graph, node_map }
    }

    /// 图中任务数量。
    pub fn len(&self) -> usize {
        self.graph.node_count()
    }

    /// 是否为空图。
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// 按 ID 查找任务。
    pub fn get(&self, id: &TaskId) -> Option<&Task> {
        self.node_map.get(id).map(|&idx| &self.graph[idx])
    }

    /// Kahn 拓扑排序，按层分组。
    ///
    /// 返回 `Vec<Vec<TaskId>>`：每个内层 `Vec` 中的任务互不依赖，可并行执行。
    /// 若存在环则返回 [`SchedulerError::CycleDetected`]。
    pub fn topo_layers(&self) -> Result<Vec<Vec<TaskId>>, SchedulerError> {
        let mut in_degree: HashMap<NodeIndex, usize> = HashMap::new();

        for idx in self.graph.node_indices() {
            let degree = self
                .graph
                .neighbors_directed(idx, Direction::Incoming)
                .count();
            in_degree.insert(idx, degree);
        }

        let mut queue: VecDeque<NodeIndex> = in_degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(&idx, _)| idx)
            .collect();

        // 稳定层内顺序：按 TaskId 排序，避免 HashMap 迭代顺序导致 flaky 测试。
        queue.make_contiguous().sort_by_key(|idx| self.graph[*idx].id);

        let mut layers: Vec<Vec<TaskId>> = Vec::new();
        let mut processed = 0usize;

        while !queue.is_empty() {
            let layer_size = queue.len();
            let mut current_layer = Vec::with_capacity(layer_size);

            for _ in 0..layer_size {
                let Some(idx) = queue.pop_front() else {
                    break;
                };
                current_layer.push(self.graph[idx].id);
                processed += 1;

                let mut newly_ready = Vec::new();
                for neighbor in self.graph.neighbors_directed(idx, Direction::Outgoing) {
                    let Some(deg) = in_degree.get_mut(&neighbor) else {
                        continue;
                    };
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        newly_ready.push(neighbor);
                    }
                }
                newly_ready.sort_by_key(|n| self.graph[*n].id);
                queue.extend(newly_ready);
            }

            layers.push(current_layer);
        }

        if processed != self.graph.node_count() {
            return Err(SchedulerError::CycleDetected);
        }

        Ok(layers)
    }
}

/// DAG 并行任务调度器。
#[derive(Debug, Clone)]
pub struct Scheduler {
    /// 单层内最大并行数（chunk 大小）。
    pub max_parallel: usize,
    /// 允许的最大层深度索引（0 基；层数 = depth + 1）。
    pub max_depth: u32,
}

impl Scheduler {
    /// 创建调度器。
    ///
    /// # Errors
    ///
    /// `max_parallel == 0` 时返回 [`SchedulerError::InvalidConfig`]。
    pub fn new(max_parallel: usize, max_depth: u32) -> Result<Self, SchedulerError> {
        if max_parallel == 0 {
            return Err(SchedulerError::InvalidConfig(
                "max_parallel must be >= 1".to_string(),
            ));
        }
        Ok(Self {
            max_parallel,
            max_depth,
        })
    }

    /// 执行任务图：按拓扑层调度，层内按 `max_parallel` 分块并行。
    ///
    /// `execute` 接收待执行任务，返回异步 `Outcome`。
    ///
    /// # Errors
    ///
    /// - 图有环 → [`SchedulerError::CycleDetected`]
    /// - 层深度超过 `max_depth` → [`SchedulerError::MaxDepthExceeded`]
    pub async fn run<F, Fut>(
        &self,
        graph: &TaskGraph,
        mut execute: F,
    ) -> Result<Vec<Outcome>, SchedulerError>
    where
        F: FnMut(&Task) -> Fut,
        Fut: Future<Output = Outcome>,
    {
        let layers = graph.topo_layers()?;
        let mut all_outcomes = Vec::new();

        info!(
            layers = layers.len(),
            tasks = graph.len(),
            max_parallel = self.max_parallel,
            max_depth = self.max_depth,
            "scheduler starting"
        );

        for (depth, layer) in layers.iter().enumerate() {
            let depth_u32 = u32::try_from(depth).map_err(|_| SchedulerError::MaxDepthExceeded {
                depth: u32::MAX,
                max_depth: self.max_depth,
            })?;

            if depth_u32 > self.max_depth {
                return Err(SchedulerError::MaxDepthExceeded {
                    depth: depth_u32,
                    max_depth: self.max_depth,
                });
            }

            debug!(depth = depth_u32, layer_size = layer.len(), "running layer");

            for chunk in layer.chunks(self.max_parallel) {
                let mut futures = Vec::with_capacity(chunk.len());
                for task_id in chunk {
                    let Some(task) = graph.get(task_id) else {
                        return Err(SchedulerError::MissingTask(*task_id));
                    };
                    futures.push(execute(task));
                }
                let results = join_all(futures).await;
                all_outcomes.extend(results);
            }
        }

        info!(outcomes = all_outcomes.len(), "scheduler finished");
        Ok(all_outcomes)
    }
}

/// 调度器错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SchedulerError {
    /// 调度层深度超过限制。
    #[error("max scheduling depth exceeded: depth {depth} > max_depth {max_depth}")]
    MaxDepthExceeded {
        /// 实际层索引。
        depth: u32,
        /// 配置的最大深度。
        max_depth: u32,
    },

    /// 任务图存在环。
    #[error("cycle detected in task graph")]
    CycleDetected,

    /// 无效配置。
    #[error("invalid scheduler config: {0}")]
    InvalidConfig(String),

    /// 拓扑层中的任务在图中找不到（内部一致性错误）。
    #[error("task missing from graph: {0}")]
    MissingTask(TaskId),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(kind: TaskKind, instruction: &str) -> Task {
        Task::new(kind, instruction)
    }

    fn task_with_deps(kind: TaskKind, instruction: &str, deps: Vec<TaskId>) -> Task {
        let mut t = Task::new(kind, instruction);
        t.deps = deps;
        t
    }

    #[test]
    fn test_topo_sort_independent_tasks_single_layer() {
        let a = task(TaskKind::Explore, "a");
        let b = task(TaskKind::Plan, "b");
        let a_id = a.id;
        let b_id = b.id;

        let graph = TaskGraph::from_tasks(vec![a, b]);
        let layers = graph.topo_layers().expect("no cycle");

        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].len(), 2);
        assert!(layers[0].contains(&a_id));
        assert!(layers[0].contains(&b_id));
    }

    #[test]
    fn test_topo_layers_respects_dependencies() {
        let explore = task(TaskKind::Explore, "scan");
        let plan = task_with_deps(TaskKind::Plan, "plan", vec![explore.id]);
        let exec = task_with_deps(TaskKind::Execute, "do", vec![plan.id]);

        let explore_id = explore.id;
        let plan_id = plan.id;
        let exec_id = exec.id;

        let graph = TaskGraph::from_tasks(vec![explore, plan, exec]);
        let layers = graph.topo_layers().expect("no cycle");

        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![explore_id]);
        assert_eq!(layers[1], vec![plan_id]);
        assert_eq!(layers[2], vec![exec_id]);
    }

    #[test]
    fn test_topo_layers_parallel_middle_layer() {
        // explore → {plan_a, plan_b} → exec
        let explore = task(TaskKind::Explore, "scan");
        let plan_a = task_with_deps(TaskKind::Plan, "plan-a", vec![explore.id]);
        let plan_b = task_with_deps(TaskKind::Plan, "plan-b", vec![explore.id]);
        let exec = task_with_deps(
            TaskKind::Execute,
            "merge",
            vec![plan_a.id, plan_b.id],
        );

        let explore_id = explore.id;
        let plan_a_id = plan_a.id;
        let plan_b_id = plan_b.id;
        let exec_id = exec.id;

        let graph = TaskGraph::from_tasks(vec![explore, plan_a, plan_b, exec]);
        let layers = graph.topo_layers().expect("no cycle");

        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec![explore_id]);
        assert_eq!(layers[1].len(), 2);
        assert!(layers[1].contains(&plan_a_id));
        assert!(layers[1].contains(&plan_b_id));
        assert_eq!(layers[2], vec![exec_id]);
    }

    #[test]
    fn test_empty_tasks_yields_empty_layers() {
        let graph = TaskGraph::from_tasks(vec![]);
        assert!(graph.is_empty());
        let layers = graph.topo_layers().expect("empty has no cycle");
        assert!(layers.is_empty());
    }

    #[test]
    fn test_cycle_detected() {
        let id_a = TaskId::new_v4();
        let id_b = TaskId::new_v4();
        let a = Task::with_id(id_a, TaskKind::Execute, "a", vec![id_b]);
        let b = Task::with_id(id_b, TaskKind::Execute, "b", vec![id_a]);

        let graph = TaskGraph::from_tasks(vec![a, b]);
        let err = graph.topo_layers().expect_err("cycle");
        assert_eq!(err, SchedulerError::CycleDetected);
    }

    #[test]
    fn test_scheduler_rejects_zero_max_parallel() {
        let err = Scheduler::new(0, 10).expect_err("zero parallel");
        assert!(matches!(err, SchedulerError::InvalidConfig(_)));
    }

    #[tokio::test]
    async fn test_run_depth_limit() {
        let a = task(TaskKind::Explore, "a");
        let b = task_with_deps(TaskKind::Plan, "b", vec![a.id]);
        let c = task_with_deps(TaskKind::Execute, "c", vec![b.id]);
        // 3 层（depth 0,1,2）；max_depth=1 应在 depth=2 失败
        let graph = TaskGraph::from_tasks(vec![a, b, c]);
        let scheduler = Scheduler::new(4, 1).expect("valid");

        let result = scheduler
            .run(&graph, |t| {
                let id = t.id;
                async move {
                    Outcome {
                        task_id: id,
                        success: true,
                        quality: 8.0,
                        latency_ms: 1,
                        cost_usd: 0.0,
                    }
                }
            })
            .await;

        let err = result.expect_err("depth exceeded");
        assert!(matches!(
            err,
            SchedulerError::MaxDepthExceeded {
                depth: 2,
                max_depth: 1
            }
        ));
    }

    #[tokio::test]
    async fn test_run_executes_all_tasks_in_order() {
        let explore = task(TaskKind::Explore, "scan");
        let plan = task_with_deps(TaskKind::Plan, "plan", vec![explore.id]);
        let exec = task_with_deps(TaskKind::Execute, "do", vec![plan.id]);

        let explore_id = explore.id;
        let plan_id = plan.id;
        let exec_id = exec.id;

        let graph = TaskGraph::from_tasks(vec![explore, plan, exec]);
        let scheduler = Scheduler::new(2, 10).expect("valid");

        let outcomes = scheduler
            .run(&graph, |t| {
                let id = t.id;
                async move {
                    Outcome {
                        task_id: id,
                        success: true,
                        quality: 9.0,
                        latency_ms: 10,
                        cost_usd: 0.001,
                    }
                }
            })
            .await
            .expect("run ok");

        assert_eq!(outcomes.len(), 3);
        assert_eq!(outcomes[0].task_id, explore_id);
        assert_eq!(outcomes[1].task_id, plan_id);
        assert_eq!(outcomes[2].task_id, exec_id);
        assert!(outcomes.iter().all(|o| o.success));
    }

    #[tokio::test]
    async fn test_run_empty_graph() {
        let graph = TaskGraph::from_tasks(vec![]);
        let scheduler = Scheduler::new(1, 0).expect("valid");
        let outcomes = scheduler
            .run(&graph, |_t| async {
                Outcome {
                    task_id: TaskId::nil(),
                    success: false,
                    quality: 0.0,
                    latency_ms: 0,
                    cost_usd: 0.0,
                }
            })
            .await
            .expect("empty ok");
        assert!(outcomes.is_empty());
    }
}
