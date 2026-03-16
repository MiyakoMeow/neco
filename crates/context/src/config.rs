//! Context configuration module.

use serde::{Deserialize, Serialize};

/// Configuration for context management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Whether auto-compaction is enabled.
    pub auto_compact_enabled: bool,
    /// Threshold for auto-compaction.
    pub auto_compact_threshold: f64,
    /// Model group to use for compaction.
    pub compact_model_group: ModelGroupRef,
    /// Number of recent messages to keep.
    pub keep_recent_messages: usize,
    /// Context window size in tokens.
    pub context_window_tokens: usize,
    /// Goldilocks zone minimum threshold.
    pub goldilocks_min: f64,
    /// Goldilocks zone maximum threshold.
    pub goldilocks_max: f64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            auto_compact_enabled: true,
            auto_compact_threshold: 0.9,
            compact_model_group: ModelGroupRef::new("gpt-4o-mini"),
            keep_recent_messages: 10,
            context_window_tokens: 128 * 1024,
            goldilocks_min: 0.4,
            goldilocks_max: 0.7,
        }
    }
}

impl ContextConfig {
    /// Checks if the usage is in the goldilocks zone.
    pub fn is_goldilocks_zone(&self, usage_percent: f64) -> bool {
        let percent = usage_percent / 100.0;
        percent >= self.goldilocks_min && percent <= self.goldilocks_max
    }

    /// Gets the token threshold for auto-compaction.
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn auto_compact_threshold_tokens(&self) -> usize {
        (self.context_window_tokens as f64 * self.auto_compact_threshold) as usize
    }

    /// Gets the minimum tokens for the goldilocks zone.
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn goldilocks_zone_min_tokens(&self) -> usize {
        (self.context_window_tokens as f64 * self.goldilocks_min) as usize
    }

    /// Gets the maximum tokens for the goldilocks zone.
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    pub fn goldilocks_zone_max_tokens(&self) -> usize {
        (self.context_window_tokens as f64 * self.goldilocks_max) as usize
    }
}

/// Reference to a model group.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelGroupRef(String);

impl ModelGroupRef {
    /// Creates a new `ModelGroupRef`.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Gets the model group as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ModelGroupRef {
    fn default() -> Self {
        Self::new("gpt-4o-mini")
    }
}

/// Token savings information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSavings {
    /// Tokens before compression.
    pub before: u32,
    /// Tokens after compression.
    pub after: u32,
    /// Tokens saved.
    pub saved: u32,
    /// Percentage saved.
    pub saved_percent: f64,
}

impl TokenSavings {
    /// Creates new `TokenSavings`.
    pub fn new(before: u32, after: u32) -> Self {
        let saved = before.saturating_sub(after);
        let saved_percent = if before > 0 {
            (f64::from(saved) / f64::from(before)) * 100.0
        } else {
            0.0
        };
        Self {
            before,
            after,
            saved,
            saved_percent,
        }
    }
}
