//! kimix-core: Lightweight BM25 retrieval engine with CJK-aware n-gram tokenization.
//!
//! # Architecture
//!
//! ```text
//! Text → Tokenizer (CJK bigram + whitespace) → InvertedIndex → BM25Scorer → Searcher
//!                                                                              │
//!                                                                     MMRReranker (optional)
//! ```
//!
//! # Example
//!
//! ```rust
//! use kimix_core::{Tokenizer, InvertedIndex, BM25Scorer, Searcher};
//!
//! let mut tokenizer = Tokenizer::new(2);
//! let scorer = BM25Scorer::default();
//! let searcher = Searcher::new(tokenizer.clone(), scorer);
//!
//! let mut index = InvertedIndex::new();
//! index.add_document(0, &tokenizer.tokenize("异步HTTP客户端"));
//! index.add_document(1, &tokenizer.tokenize("Rust TUI应用"));
//!
//! let results = searcher.search("HTTP客户端", &index, 5);
//! ```
pub mod agent_scheduler;
pub mod cache_engine;
pub mod fork_agent;
pub mod hybrid;
pub mod index;
pub mod model_router;
pub mod plan_mode;
pub mod recall;
pub mod safety;
pub mod scorer;
pub mod searcher;
pub mod subagent;
pub mod tokenizer;
pub mod vector;

// Re-export main types
pub use agent_scheduler::{
    Outcome, Scheduler, SchedulerError, Task, TaskGraph, TaskId, TaskKind,
};
pub use fork_agent::{
    check_fork_safety, fork, try_fork, ForkError, SessionRole, SessionStatus,
    FORK_BOILERPLATE_TAG, MAX_FORK_DEPTH,
};
pub use hybrid::HybridSearcher;
pub use index::InvertedIndex;
pub use plan_mode::{
    approve_plan, default_plans_dir, enter_plan, exit_plan, generate_plan_path, AgentSession,
    AllowedPrompt, PermissionMode, PlanContext, PlanError,
};
pub use recall::{RecallConfig, RecallEngine, RecallResult, RecallTier};
pub use safety::{sanitize_response, PathGuard};
pub use scorer::BM25Scorer;
pub use searcher::{MMRReranker, SearchResult, Searcher};
pub use subagent::{AgentRole, CapabilityMode, SubagentConfig, SubagentResult};
pub use tokenizer::Tokenizer;
pub use vector::VectorIndex;
