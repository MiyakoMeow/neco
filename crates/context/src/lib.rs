//! `NeoCo` Context Management Module
//!
//! This module provides context management for LLM conversations,
//! implementing the Arena Allocator mental model for context windows.

#![allow(clippy::missing_panics_doc)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::must_use_candidate)]
#![allow(unused_crate_dependencies)]

pub mod compression;
pub mod config;
pub mod manager;
pub mod observer;
pub mod tokenizer;

pub use compression::*;
pub use config::*;
pub use manager::*;
pub use observer::*;
pub use tokenizer::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_group_ref() {
        let model_ref = ModelGroupRef::new("gpt-4");
        assert_eq!(model_ref.as_str(), "gpt-4");
    }

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert!(config.auto_compact_enabled);
        assert!((config.auto_compact_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(config.context_window_tokens, 128 * 1024);
    }

    #[test]
    fn test_token_savings() {
        let savings = TokenSavings::new(1000, 300);
        assert_eq!(savings.before, 1000);
        assert_eq!(savings.after, 300);
        assert_eq!(savings.saved, 700);
        assert!((savings.saved_percent - 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pruning_stage_order() {
        use crate::observer::PruningStage;
        assert!(PruningStage::Stage1SoftTrim < PruningStage::Stage2HardClear);
        assert!(PruningStage::Stage2HardClear < PruningStage::Stage3Graded);
    }

    #[test]
    fn test_goldilocks_zone() {
        let config = ContextConfig::default();
        assert!(config.is_goldilocks_zone(50.0));
        assert!(config.is_goldilocks_zone(41.0));
        assert!(config.is_goldilocks_zone(69.0));
        assert!(!config.is_goldilocks_zone(30.0));
        assert!(!config.is_goldilocks_zone(80.0));
    }
}
