//! Model capabilities registry

use crate::types::ModelCapabilities;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

/// Key for looking up model capabilities.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ModelCapabilityKey {
    /// The provider name.
    pub provider: String,
    /// The model name.
    pub model: String,
}

/// Model capabilities registry.
static MODEL_CAPABILITIES: LazyLock<RwLock<HashMap<ModelCapabilityKey, ModelCapabilities>>> =
    LazyLock::new(|| {
        let mut m = HashMap::new();

        // OpenAI - GPT-4o series
        m.insert(
            ModelCapabilityKey {
                provider: "openai".into(),
                model: "gpt-4o".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "openai".into(),
                model: "gpt-4o-mini".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );

        // OpenAI - o1 series
        m.insert(
            ModelCapabilityKey {
                provider: "openai".into(),
                model: "o1".into(),
            },
            ModelCapabilities {
                streaming: false,
                tools: false,
                functions: false,
                json_mode: false,
                vision: true,
                context_window: 200_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "openai".into(),
                model: "o1-mini".into(),
            },
            ModelCapabilities {
                streaming: false,
                tools: false,
                functions: false,
                json_mode: false,
                vision: false,
                context_window: 128_000,
            },
        );

        // OpenAI - GPT-3.5 series
        m.insert(
            ModelCapabilityKey {
                provider: "openai".into(),
                model: "gpt-3.5-turbo".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: false,
                context_window: 16_385,
            },
        );

        // Zhipu (智谱) models
        m.insert(
            ModelCapabilityKey {
                provider: "zhipuai".into(),
                model: "glm-4.7".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "zhipu".into(),
                model: "glm-4".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: false,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "zhipu".into(),
                model: "glm-4-flash".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "zhipu".into(),
                model: "glm-4-plus".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "zhipu".into(),
                model: "glm-4-vision".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 4_096,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "zhipu".into(),
                model: "glm-3-turbo".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: false,
                context_window: 32_768,
            },
        );

        // MiniMax models
        m.insert(
            ModelCapabilityKey {
                provider: "minimax".into(),
                model: "abab6.5s-chat".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 245_760,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "minimax".into(),
                model: "abab6.5g-chat".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 245_760,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "minimax".into(),
                model: "abab5.5s-chat".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: false,
                context_window: 32_768,
            },
        );

        // Anthropic models
        m.insert(
            ModelCapabilityKey {
                provider: "anthropic".into(),
                model: "claude-3-5-sonnet-20241022".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 200_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "anthropic".into(),
                model: "claude-3-opus-20240229".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 200_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "anthropic".into(),
                model: "claude-3-sonnet-20240229".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 200_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "anthropic".into(),
                model: "claude-3-haiku-20240307".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 200_000,
            },
        );

        // OpenRouter models (common ones)
        m.insert(
            ModelCapabilityKey {
                provider: "openrouter".into(),
                model: "anthropic/claude-3.5-sonnet".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 200_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "openrouter".into(),
                model: "openai/gpt-4o".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 128_000,
            },
        );
        m.insert(
            ModelCapabilityKey {
                provider: "openrouter".into(),
                model: "google/gemini-pro-1.5".into(),
            },
            ModelCapabilities {
                streaming: true,
                tools: true,
                functions: true,
                json_mode: true,
                vision: true,
                context_window: 2_000_000,
            },
        );

        RwLock::new(m)
    });

/// Registry for model capabilities.
pub struct CapabilitiesRegistry;

impl CapabilitiesRegistry {
    /// Get model capabilities by provider and model name.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock cannot be acquired.
    pub fn get(provider: &str, model: &str) -> ModelCapabilities {
        let key = ModelCapabilityKey {
            provider: provider.into(),
            model: model.into(),
        };
        MODEL_CAPABILITIES
            .read()
            .unwrap()
            .get(&key)
            .cloned()
            .unwrap_or_else(ModelCapabilities::default)
    }

    /// Register model capabilities.
    ///
    /// # Panics
    ///
    /// Panics if the internal lock cannot be acquired.
    pub fn register(key: ModelCapabilityKey, capabilities: ModelCapabilities) {
        let mut m = MODEL_CAPABILITIES.write().unwrap();
        m.insert(key, capabilities);
    }
}
