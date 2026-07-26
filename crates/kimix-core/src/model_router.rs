//! 模型路由器：基于 UCB 多臂老虎机的动态模型选择。
//!
//! # 算法
//!
//! ```text
//! score(model) = exploit(model) + explore(model)
//!              = mean_quality     + C * sqrt(ln(N) / n)
//!
//! 其中:
//!   mean_quality = 该模型在相关任务上的历史平均质量
//!   C            = 探索权重（配置项，默认 1.414）
//!   N            = 所有模型的总观测次数
//!   n            = 该模型的观测次数
//! ```
//!
//! # 学习循环
//!
//! ```text
//! Task → Route → Execute → Measure → Learn
//!   ↑                                    │
//!   └────────────────────────────────────┘
//! ```

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tracing::{debug, info, warn};

/// 贝叶斯信念：从数据中学习的模型能力估计。
///
/// 使用均值 / 标准差 / 样本数三元组表示后验分布，
/// 通过 Welford 在线算法在每次观测后更新。
///
/// # Examples
///
/// ```
/// use kimix_core::model_router::Belief;
///
/// let mut belief = Belief::prior();
/// belief.update(8.0);
/// belief.update(9.0);
/// assert_eq!(belief.samples, 2);
/// assert!(belief.mean > 5.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Belief {
    /// 后验均值（能力评分 0–10）
    pub mean: f64,
    /// 后验标准差（不确定性）
    pub std: f64,
    /// 观测次数
    pub samples: u64,
}

impl Belief {
    /// 冷启动先验：无信息先验（中等评分 + 高不确定性）。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::model_router::Belief;
    ///
    /// let prior = Belief::prior();
    /// assert_eq!(prior.mean, 5.0);
    /// assert_eq!(prior.samples, 0);
    /// assert!(prior.std > 0.0);
    /// ```
    pub fn prior() -> Self {
        Self {
            mean: 5.0,
            std: 5.0,
            samples: 0,
        }
    }

    /// 从单个观测创建信念。
    ///
    /// 单次观测仍保留较大不确定性（`std = 2.5`）。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::model_router::Belief;
    ///
    /// let b = Belief::from_observation(8.0);
    /// assert_eq!(b.mean, 8.0);
    /// assert_eq!(b.samples, 1);
    /// ```
    pub fn from_observation(obs: f64) -> Self {
        Self {
            mean: obs,
            std: 2.5,
            samples: 1,
        }
    }

    /// 贝叶斯更新：新观测后更新信念。
    ///
    /// 使用 Welford 在线算法计算均值和方差。
    /// 当 `samples == 0`（仍为先验）时，退化为 [`from_observation`]。
    ///
    /// # Examples
    ///
    /// ```
    /// use kimix_core::model_router::Belief;
    ///
    /// let mut belief = Belief::prior();
    /// belief.update(8.0);
    /// belief.update(9.0);
    /// belief.update(7.0);
    /// assert_eq!(belief.samples, 3);
    /// assert!((belief.mean - 8.0).abs() < 0.1);
    /// ```
    pub fn update(&mut self, observation: f64) {
        if self.samples == 0 {
            *self = Self::from_observation(observation);
            return;
        }

        let n = self.samples as f64;

        // 在线均值更新
        let new_mean = self.mean + (observation - self.mean) / (n + 1.0);

        // 在线方差更新（Welford）
        let delta = observation - self.mean;
        let delta2 = observation - new_mean;
        let old_var = self.std * self.std;
        let new_var = (old_var * n + delta * delta2) / (n + 1.0);

        self.mean = new_mean;
        // 最小不确定性，避免除零与过度自信
        self.std = new_var.max(0.0).sqrt().max(0.01);
        self.samples += 1;
    }
}

/// 模型画像：可路由的模型元数据与能力信念。
///
/// # Examples
///
/// ```
/// use kimix_core::model_router::ModelProfile;
///
/// let profile = ModelProfile::new("kimi-k3", "Kimi K3", "https://api.example.com", 128_000);
/// assert_eq!(profile.id, "kimi-k3");
/// assert_eq!(profile.overall.samples, 0);
/// ```
#[derive(Debug, Clone)]
pub struct ModelProfile {
    /// 模型唯一标识
    pub id: String,
    /// 人类可读名称
    pub display_name: String,
    /// API 基础 URL
    pub base_url: String,
    /// 认证环境变量名
    pub env_key: Option<String>,
    /// 上下文窗口大小
    pub context_window: u32,
    /// 能力信念（按维度）
    pub capabilities: HashMap<CapabilityDim, Belief>,
    /// 总体信念
    pub overall: Belief,
    /// 降级链（备用模型 ID 列表）
    pub fallback_chain: Vec<String>,
    /// 首次注册时间
    pub first_seen: DateTime<Utc>,
    /// 最后使用时间
    pub last_used: DateTime<Utc>,
}

impl ModelProfile {
    /// 使用冷启动先验创建新模型画像。
    ///
    /// # Arguments
    ///
    /// * `id` - 模型唯一标识
    /// * `display_name` - 人类可读名称
    /// * `base_url` - API 基础 URL
    /// * `context_window` - 上下文窗口 token 数
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        base_url: impl Into<String>,
        context_window: u32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            display_name: display_name.into(),
            base_url: base_url.into(),
            env_key: None,
            context_window,
            capabilities: HashMap::new(),
            overall: Belief::prior(),
            fallback_chain: Vec::new(),
            first_seen: now,
            last_used: now,
        }
    }
}

/// 能力维度。
///
/// 用于描述任务需求与模型专长匹配。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityDim {
    /// 推理 / 规划
    Reasoning,
    /// 代码生成与修复
    Coding,
    /// 写作 / 润色
    Writing,
    /// 检索与调研
    Search,
    /// 视觉多模态
    Vision,
    /// 中日韩语言
    Cjk,
    /// 工具调用
    ToolCalling,
    /// 响应速度
    Speed,
}

/// 路由权重配置。
///
/// UCB 探索项使用 `exploration`（默认 √2 ≈ 1.414）。
///
/// # Examples
///
/// ```
/// use kimix_core::model_router::RoutingWeights;
///
/// let w = RoutingWeights::default();
/// assert!((w.exploration - 1.414).abs() < 0.001);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct RoutingWeights {
    /// 能力匹配权重
    pub capability: f64,
    /// 成本权重
    pub cost: f64,
    /// 延迟权重
    pub latency: f64,
    /// 缓存支持权重
    pub cache: f64,
    /// 历史表现权重
    pub learned: f64,
    /// UCB 探索权重（C）
    pub exploration: f64,
}

impl Default for RoutingWeights {
    fn default() -> Self {
        Self {
            capability: 0.40,
            cost: 0.25,
            latency: 0.15,
            cache: 0.10,
            learned: 0.10,
            exploration: 1.414, // sqrt(2)
        }
    }
}

/// UCB 多臂老虎机模型路由器。
///
/// 在注册的模型集合上按任务上下文选择最优模型，
/// 并通过 [`learn`](Router::learn) 从执行结果更新信念。
///
/// # Examples
///
/// ```
/// use kimix_core::model_router::{
///     ModelProfile, QualityHint, Router, RoutingWeights, TaskContext,
/// };
///
/// let mut router = Router::new(RoutingWeights::default());
/// router.register(ModelProfile::new("a", "A", "https://api.example.com", 100_000));
///
/// let task = TaskContext {
///     instruction: "refactor module".into(),
///     required_caps: vec![],
///     min_context_window: None,
///     quality_hint: QualityHint::Balanced,
/// };
/// let route = router.route(&task).expect("should select a model");
/// assert_eq!(route.model_id, "a");
/// ```
pub struct Router {
    /// 已注册模型（id → 画像）
    pub models: HashMap<String, ModelProfile>,
    /// 路由权重
    pub weights: RoutingWeights,
}

impl Router {
    /// 创建空路由器。
    pub fn new(weights: RoutingWeights) -> Self {
        Self {
            models: HashMap::new(),
            weights,
        }
    }

    /// 注册模型画像。同 id 会覆盖既有条目。
    pub fn register(&mut self, profile: ModelProfile) {
        info!(model_id = %profile.id, "registering model profile");
        self.models.insert(profile.id.clone(), profile);
    }

    /// 核心路由：按 UCB 得分选择模型。
    ///
    /// # Arguments
    ///
    /// * `task` - 任务上下文（能力需求、上下文窗口、质量偏好）
    ///
    /// # Returns
    ///
    /// 路由结果（模型 ID + 得分 + 降级链）。
    ///
    /// # Errors
    ///
    /// * [`RouterError::NoModelsRegistered`] — 尚未注册任何模型
    /// * [`RouterError::NoModelFound`] — 过滤后无候选模型
    pub fn route(&self, task: &TaskContext) -> Result<Route, RouterError> {
        if self.models.is_empty() {
            warn!("route called with no models registered");
            return Err(RouterError::NoModelsRegistered);
        }

        // 1. 过滤：基本能力匹配
        let candidates: Vec<&ModelProfile> = self
            .models
            .values()
            .filter(|m| self.meets_requirements(m, task))
            .collect();

        if candidates.is_empty() {
            return Err(RouterError::NoModelFound(format!(
                "{:?}",
                task.required_caps
            )));
        }

        // 2. 计算 UCB 得分：score = mean + C * sqrt(ln(N) / n)
        let total_n: u64 = candidates.iter().map(|m| m.overall.samples).sum();
        // total_n == 0 时 ln 为 -inf，用 max(1.0) 保证探索项有意义
        let log_total = (total_n as f64).ln().max(1.0);

        let mut scored: Vec<Route> = candidates
            .iter()
            .map(|m| {
                let raw = self.ucb_score(m, log_total);
                let score = self.apply_quality_hint(raw, task.quality_hint, m);
                Route {
                    model_id: m.id.clone(),
                    score,
                    fallback_chain: m.fallback_chain.clone(),
                }
            })
            .collect();

        // 3. 降序排序（NaN 视作相等，避免 panic）
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // candidates 非空 ⇒ scored 非空
        let best = scored
            .into_iter()
            .next()
            .ok_or_else(|| RouterError::NoModelFound(format!("{:?}", task.required_caps)))?;

        debug!(
            model_id = %best.model_id,
            score = best.score,
            total_n,
            "selected model via UCB"
        );

        Ok(best)
    }

    /// 学习：根据执行结果更新模型总体信念。
    ///
    /// 未知 `model_id` 时静默忽略（不 panic）。
    pub fn learn(&mut self, model_id: &str, outcome: &Outcome) {
        match self.models.get_mut(model_id) {
            Some(profile) => {
                profile.overall.update(outcome.quality);
                profile.last_used = Utc::now();
                debug!(
                    model_id,
                    quality = outcome.quality,
                    samples = profile.overall.samples,
                    mean = profile.overall.mean,
                    "updated model belief"
                );
            }
            None => {
                warn!(model_id, "learn called for unknown model");
            }
        }
    }

    /// UCB 原始得分：`mean + C * sqrt(ln(N) / n)`。
    fn ucb_score(&self, model: &ModelProfile, log_total: f64) -> f64 {
        let exploit = model.overall.mean;
        let n = model.overall.samples.max(1) as f64;
        let explore = self.weights.exploration * (log_total / n).sqrt();
        exploit + explore
    }

    /// 按质量偏好微调得分。
    fn apply_quality_hint(&self, raw_score: f64, hint: QualityHint, model: &ModelProfile) -> f64 {
        match hint {
            QualityHint::Balanced => raw_score,
            // 速度偏好：上下文越大通常越慢，用窗口倒数作弱代理
            QualityHint::Fast => {
                let speed_proxy = 1.0 / (1.0 + (model.context_window as f64 / 100_000.0).ln_1p());
                raw_score * (0.5 + 0.5 * speed_proxy)
            }
            // 成本偏好：暂用样本多（更确定）的廉价启发式，保留 raw 主导
            QualityHint::Cheap => raw_score * 0.95,
            // 质量偏好：放大高均值模型差距
            QualityHint::Quality => raw_score * (0.8 + 0.04 * model.overall.mean),
        }
    }

    /// 检查模型是否满足任务基本要求。
    fn meets_requirements(&self, model: &ModelProfile, task: &TaskContext) -> bool {
        if let Some(min_ctx) = task.min_context_window
            && model.context_window < min_ctx
        {
            return false;
        }
        // 若任务声明了能力维度且模型已有对应信念，要求 mean ≥ 3.0
        // 冷启动（无该维度记录）放行，交给 UCB 探索
        for cap in &task.required_caps {
            if let Some(belief) = model.capabilities.get(cap)
                && belief.samples > 0
                && belief.mean < 3.0
            {
                return false;
            }
        }
        true
    }
}

/// 路由结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// 选定模型 ID
    pub model_id: String,
    /// UCB（经质量偏好调整后）得分
    pub score: f64,
    /// 降级链
    pub fallback_chain: Vec<String>,
}

/// 路由器错误。
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// 尚未注册任何模型
    #[error("no models registered")]
    NoModelsRegistered,

    /// 过滤后无满足要求的模型
    #[error("no model found for requirements: {0}")]
    NoModelFound(String),
}

/// 任务上下文。
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// 任务指令文本
    pub instruction: String,
    /// 所需能力维度
    pub required_caps: Vec<CapabilityDim>,
    /// 最小上下文窗口（token）
    pub min_context_window: Option<u32>,
    /// 质量 / 成本 / 速度偏好
    pub quality_hint: QualityHint,
}

/// 质量偏好提示。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityHint {
    /// 优先低延迟
    Fast,
    /// 优先低成本
    Cheap,
    /// 优先高质量
    Quality,
    /// 平衡（默认）
    #[default]
    Balanced,
}

/// 任务执行结果，用于信念更新。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 是否成功完成
    pub success: bool,
    /// 质量评分（0–10）
    pub quality: f64,
    /// 延迟（毫秒）
    pub latency_ms: u64,
    /// 成本（美元）
    pub cost_usd: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_profile(id: &str, mean: f64, samples: u64, ctx: u32) -> ModelProfile {
        let mut p = ModelProfile::new(id, id, "https://api.example.com", ctx);
        p.overall = Belief {
            mean,
            std: 0.5,
            samples,
        };
        p
    }

    fn balanced_task() -> TaskContext {
        TaskContext {
            instruction: "test".into(),
            required_caps: vec![],
            min_context_window: None,
            quality_hint: QualityHint::Balanced,
        }
    }

    #[test]
    fn test_belief_prior() {
        let prior = Belief::prior();
        assert_eq!(prior.mean, 5.0);
        assert_eq!(prior.std, 5.0);
        assert_eq!(prior.samples, 0);
    }

    #[test]
    fn test_belief_from_observation() {
        let b = Belief::from_observation(8.0);
        assert_eq!(b.mean, 8.0);
        assert_eq!(b.std, 2.5);
        assert_eq!(b.samples, 1);
    }

    #[test]
    fn test_belief_update() {
        let mut belief = Belief::prior();
        belief.update(8.0);
        belief.update(9.0);
        belief.update(7.0);

        assert_eq!(belief.samples, 3);
        assert!((belief.mean - 8.0).abs() < 0.1);
        assert!(belief.std > 0.0);
    }

    #[test]
    fn test_belief_update_variance_decreases_with_consistent_obs() {
        let mut belief = Belief::from_observation(7.0);
        let initial_std = belief.std;
        for _ in 0..50 {
            belief.update(7.0);
        }
        assert!(belief.std < initial_std);
        assert!((belief.mean - 7.0).abs() < 0.01);
    }

    #[test]
    fn test_route_selects_highest_scored() {
        let mut router = Router::new(RoutingWeights::default());
        let mut model_a = make_profile("model-a", 8.0, 100, 100_000);
        model_a.fallback_chain = vec!["fallback".into()];
        let model_b = make_profile("model-b", 6.0, 100, 100_000);

        router.register(model_a);
        router.register(model_b);

        let route = router.route(&balanced_task()).expect("route ok");
        assert_eq!(route.model_id, "model-a");
        assert_eq!(route.fallback_chain, vec!["fallback".to_string()]);
    }

    #[test]
    fn test_route_no_models() {
        let router = Router::new(RoutingWeights::default());
        let err = router.route(&balanced_task()).expect_err("should fail");
        assert!(matches!(err, RouterError::NoModelsRegistered));
    }

    #[test]
    fn test_route_no_candidates_context_window() {
        let mut router = Router::new(RoutingWeights::default());
        router.register(make_profile("small", 9.0, 50, 4_000));

        let task = TaskContext {
            instruction: "need big ctx".into(),
            required_caps: vec![],
            min_context_window: Some(100_000),
            quality_hint: QualityHint::Balanced,
        };

        let err = router.route(&task).expect_err("should fail");
        assert!(matches!(err, RouterError::NoModelFound(_)));
    }

    #[test]
    fn test_ucb_prefers_exploration_for_low_samples() {
        // 高均值但样本极多 vs 略低均值但样本极少 → 探索项应抬高后者
        let mut router = Router::new(RoutingWeights {
            exploration: 2.0, // 加大探索
            ..RoutingWeights::default()
        });

        // exploit=9, n=10000 → explore 很小
        router.register(make_profile("proven", 9.0, 10_000, 100_000));
        // exploit=7, n=1 → explore 很大
        router.register(make_profile("newcomer", 7.0, 1, 100_000));

        let route = router.route(&balanced_task()).expect("route ok");
        // 在强探索下，newcomer 的 UCB 应超过 proven
        // score_proven ≈ 9 + 2*sqrt(ln(10001)/10000) ≈ 9.02
        // score_new   ≈ 7 + 2*sqrt(ln(10001)/1) ≈ 7 + 2*9.2 ≈ 25
        assert_eq!(route.model_id, "newcomer");
    }

    #[test]
    fn test_learn_updates_belief() {
        let mut router = Router::new(RoutingWeights::default());
        router.register(make_profile("m1", 5.0, 1, 100_000));

        let outcome = Outcome {
            success: true,
            quality: 9.0,
            latency_ms: 100,
            cost_usd: 0.01,
        };
        router.learn("m1", &outcome);

        let profile = router.models.get("m1").expect("model exists");
        assert_eq!(profile.overall.samples, 2);
        assert!(profile.overall.mean > 5.0);
    }

    #[test]
    fn test_learn_unknown_model_is_noop() {
        let mut router = Router::new(RoutingWeights::default());
        router.register(make_profile("m1", 5.0, 1, 100_000));
        router.learn(
            "ghost",
            &Outcome {
                success: false,
                quality: 0.0,
                latency_ms: 0,
                cost_usd: 0.0,
            },
        );
        assert_eq!(router.models.len(), 1);
        assert_eq!(router.models.get("m1").map(|m| m.overall.samples), Some(1));
    }

    #[test]
    fn test_ucb_score_formula() {
        let router = Router::new(RoutingWeights {
            exploration: 1.414,
            ..RoutingWeights::default()
        });
        let model = make_profile("x", 8.0, 100, 100_000);
        // N = 100, log_total = ln(100) ≈ 4.605
        let log_total = (100.0_f64).ln().max(1.0);
        let score = router.ucb_score(&model, log_total);
        let expected = 8.0 + 1.414 * (log_total / 100.0).sqrt();
        assert!((score - expected).abs() < 1e-9);
    }

    #[test]
    fn test_cold_start_all_priors_routes_successfully() {
        let mut router = Router::new(RoutingWeights::default());
        router.register(ModelProfile::new("a", "A", "https://a.example", 64_000));
        router.register(ModelProfile::new("b", "B", "https://b.example", 64_000));

        let route = router
            .route(&balanced_task())
            .expect("cold start should route");
        assert!(route.model_id == "a" || route.model_id == "b");
        assert!(route.score.is_finite());
    }

    #[test]
    fn test_capability_filter_rejects_weak_dimension() {
        let mut router = Router::new(RoutingWeights::default());
        let mut weak = make_profile("weak-coder", 8.0, 10, 100_000);
        weak.capabilities
            .insert(CapabilityDim::Coding, Belief::from_observation(1.0));
        let strong = make_profile("strong-coder", 7.0, 10, 100_000);

        router.register(weak);
        router.register(strong);

        let task = TaskContext {
            instruction: "write rust".into(),
            required_caps: vec![CapabilityDim::Coding],
            min_context_window: None,
            quality_hint: QualityHint::Balanced,
        };

        let route = router.route(&task).expect("should pick strong");
        assert_eq!(route.model_id, "strong-coder");
    }
}
