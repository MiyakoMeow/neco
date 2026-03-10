//! Configuration merging module

use indexmap::IndexMap;
use smol_str::SmolStr;

use chrono::Duration;

use crate::{
    Config, ContextConfig, McpServerConfig, McpServers, ModelGroup, ModelGroups, ModelProvider,
    ModelProviders, RetryConfig, StorageConfig, SystemConfig, ToolsConfig, UiConfig,
};

/// Configuration merger
///
/// Provides functionality to merge multiple configurations.
pub struct ConfigMerger;

impl ConfigMerger {
    /// Merge two configurations.
    ///
    /// Override values will replace base values.
    #[must_use]
    pub fn merge(base: &Config, override_: &Config) -> Config {
        let model_groups = Self::merge_model_groups(&base.model_groups, &override_.model_groups);
        let model_providers =
            Self::merge_model_providers(&base.model_providers, &override_.model_providers);
        let mcp_servers = Self::merge_mcp_servers(&base.mcp_servers, &override_.mcp_servers);
        let system = Self::merge_system(&base.system, &override_.system);
        let model = override_.model.clone().or_else(|| base.model.clone());
        let model_group = override_
            .model_group
            .clone()
            .or_else(|| base.model_group.clone());

        Config {
            model,
            model_group,
            model_groups,
            model_providers,
            mcp_servers,
            system,
        }
    }

    /// Merge model groups from base and override configurations.
    fn merge_model_groups(base: &ModelGroups, override_: &ModelGroups) -> ModelGroups {
        let mut merged: IndexMap<SmolStr, ModelGroup> = base.0.clone();
        for (key, value) in override_.iter() {
            if let Some(existing) = merged.get(key.as_str()) {
                let mut combined_models = existing.models.clone();
                combined_models.extend(value.models.clone());
                combined_models.extend(value.append_models.clone());
                merged.insert(
                    key.clone(),
                    ModelGroup {
                        models: combined_models,
                        append_models: vec![],
                    },
                );
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        ModelGroups(merged)
    }

    /// Merge model providers from base and override configurations.
    fn merge_model_providers(base: &ModelProviders, override_: &ModelProviders) -> ModelProviders {
        let mut merged: IndexMap<SmolStr, ModelProvider> = base.0.clone();
        for (key, value) in override_.iter() {
            if let Some(existing) = merged.get(key.as_str()) {
                merged.insert(key.clone(), Self::merge_model_provider(existing, value));
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        ModelProviders(merged)
    }

    /// Merge a single model provider from base and override configurations.
    fn merge_model_provider(base: &ModelProvider, override_: &ModelProvider) -> ModelProvider {
        ModelProvider {
            provider_type: override_.provider_type,
            name: override_.name.clone(),
            base_url: override_.base_url.clone(),
            api_key_env: override_
                .api_key_env
                .clone()
                .or_else(|| base.api_key_env.clone()),
            api_key_envs: override_
                .api_key_envs
                .clone()
                .or_else(|| base.api_key_envs.clone()),
            api_key: override_.api_key.clone().or_else(|| base.api_key.clone()),
            default_params: Self::merge_indexmap(&base.default_params, &override_.default_params),
            retry: Self::merge_retry_config(&base.retry, &override_.retry),
            timeout: if override_.timeout == Duration::zero() {
                base.timeout
            } else {
                override_.timeout
            },
        }
    }

    /// Merge retry configuration from base and override configurations.
    fn merge_retry_config(base: &RetryConfig, override_: &RetryConfig) -> RetryConfig {
        RetryConfig {
            max_retries: override_.max_retries,
            initial_backoff: if override_.initial_backoff == Duration::zero() {
                base.initial_backoff
            } else {
                override_.initial_backoff
            },
            backoff_multiplier: override_.backoff_multiplier,
            max_backoff: if override_.max_backoff == Duration::zero() {
                base.max_backoff
            } else {
                override_.max_backoff
            },
        }
    }

    /// Merge MCP servers from base and override configurations.
    fn merge_mcp_servers(base: &McpServers, override_: &McpServers) -> McpServers {
        let mut merged: IndexMap<SmolStr, McpServerConfig> = base.0.clone();
        for (key, value) in override_.iter() {
            if let Some(existing) = merged.get(key.as_str()) {
                merged.insert(key.clone(), Self::merge_mcp_server(existing, value));
            } else {
                merged.insert(key.clone(), value.clone());
            }
        }
        McpServers(merged)
    }

    /// Merge a single MCP server configuration from base and override configurations.
    fn merge_mcp_server(base: &McpServerConfig, override_: &McpServerConfig) -> McpServerConfig {
        McpServerConfig {
            command: override_.command.clone().or_else(|| base.command.clone()),
            args: override_.args.clone().or_else(|| base.args.clone()),
            url: override_.url.clone().or_else(|| base.url.clone()),
            bearer_token_env: override_
                .bearer_token_env
                .clone()
                .or_else(|| base.bearer_token_env.clone()),
            http_headers: Self::merge_indexmap(&base.http_headers, &override_.http_headers),
            env: Self::merge_indexmap(&base.env, &override_.env),
            transport: override_
                .transport
                .clone()
                .or_else(|| base.transport.clone()),
        }
    }

    /// Merge system configuration from base and override configurations.
    fn merge_system(base: &SystemConfig, override_: &SystemConfig) -> SystemConfig {
        SystemConfig {
            storage: Self::merge_storage_config(&base.storage, &override_.storage),
            context: Self::merge_context_config(&base.context, &override_.context),
            tools: Self::merge_tools_config(&base.tools, &override_.tools),
            ui: Self::merge_ui_config(&base.ui, &override_.ui),
        }
    }

    /// Merge storage configuration from base and override configurations.
    fn merge_storage_config(_base: &StorageConfig, override_: &StorageConfig) -> StorageConfig {
        StorageConfig {
            session_dir: override_.session_dir.clone(),
            compression: override_.compression,
        }
    }

    /// Merge context configuration from base and override configurations.
    fn merge_context_config(_base: &ContextConfig, override_: &ContextConfig) -> ContextConfig {
        ContextConfig {
            auto_compact_threshold: override_.auto_compact_threshold,
            auto_compact_enabled: override_.auto_compact_enabled,
            compact_model_group: override_.compact_model_group.clone(),
            keep_recent_messages: override_.keep_recent_messages,
        }
    }

    /// Merge tools configuration from base and override configurations.
    fn merge_tools_config(base: &ToolsConfig, override_: &ToolsConfig) -> ToolsConfig {
        ToolsConfig {
            timeouts: Self::merge_indexmap(&base.timeouts, &override_.timeouts),
            default_timeout: if override_.default_timeout == Duration::zero() {
                base.default_timeout
            } else {
                override_.default_timeout
            },
        }
    }

    /// Merge UI configuration from base and override configurations.
    fn merge_ui_config(_base: &UiConfig, override_: &UiConfig) -> UiConfig {
        UiConfig {
            default_mode: override_.default_mode,
        }
    }

    /// Merge two index maps, with override values taking precedence.
    fn merge_indexmap<K, V>(base: &IndexMap<K, V>, override_: &IndexMap<K, V>) -> IndexMap<K, V>
    where
        K: std::hash::Hash + Eq + Clone,
        V: Clone,
    {
        let mut merged = base.clone();
        for (key, value) in override_ {
            merged.insert(key.clone(), value.clone());
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProviderType;
    use std::path::PathBuf;
    use url::Url;

    fn create_test_provider(name: &str) -> ModelProvider {
        ModelProvider {
            provider_type: ProviderType::OpenAI,
            name: Some(SmolStr::from(name)),
            base_url: Url::parse("https://api.example.com").unwrap(),
            api_key_env: None,
            api_key_envs: None,
            api_key: None,
            default_params: IndexMap::new(),
            retry: RetryConfig::default(),
            timeout: Duration::seconds(60),
        }
    }

    fn create_test_config() -> Config {
        let mut model_groups = ModelGroups::new();
        model_groups.insert(
            SmolStr::from("test"),
            ModelGroup {
                models: vec![],
                ..Default::default()
            },
        );

        let mut model_providers = ModelProviders::new();
        model_providers.insert(SmolStr::from("test"), create_test_provider("Test"));

        Config {
            model: None,
            model_group: None,
            model_groups,
            model_providers,
            mcp_servers: McpServers::default(),
            system: SystemConfig::default(),
        }
    }

    #[test]
    fn test_merge_model_groups_replacement() {
        let base = create_test_config();
        let mut override_ = Config::default();
        override_.model_groups.insert(
            SmolStr::from("new"),
            ModelGroup {
                models: vec![],
                ..Default::default()
            },
        );

        let merged = ConfigMerger::merge(&base, &override_);
        assert!(merged.model_groups.contains_key("new"));
    }

    #[test]
    fn test_merge_model_providers() {
        let mut base = create_test_config();
        base.model_providers
            .insert(SmolStr::from("existing"), create_test_provider("Existing"));

        let mut override_ = Config::default();
        override_
            .model_providers
            .insert(SmolStr::from("new"), create_test_provider("New"));

        let merged = ConfigMerger::merge(&base, &override_);
        assert!(merged.model_providers.contains_key("existing"));
        assert!(merged.model_providers.contains_key("new"));
    }

    #[test]
    fn test_merge_mcp_servers() {
        let base = Config::default();
        let mut override_ = Config::default();
        override_.mcp_servers.insert(
            SmolStr::from("test"),
            McpServerConfig {
                command: Some(SmolStr::from("test-command")),
                args: None,
                url: None,
                bearer_token_env: None,
                http_headers: IndexMap::new(),
                env: IndexMap::new(),
                transport: None,
            },
        );

        let merged = ConfigMerger::merge(&base, &override_);
        assert!(merged.mcp_servers.contains_key("test"));
    }

    #[test]
    fn test_merge_system_storage() {
        let base = Config::default();
        let mut override_ = Config::default();
        override_.system.storage.session_dir = PathBuf::from("/custom/path");

        let merged = ConfigMerger::merge(&base, &override_);
        assert_eq!(
            merged.system.storage.session_dir,
            PathBuf::from("/custom/path")
        );
    }

    #[test]
    fn test_merge_indexmap() {
        let mut base = IndexMap::new();
        base.insert(SmolStr::from("key1"), SmolStr::from("value1"));

        let mut override_ = IndexMap::new();
        override_.insert(SmolStr::from("key2"), SmolStr::from("value2"));

        let merged = ConfigMerger::merge_indexmap(&base, &override_);
        assert_eq!(merged.get("key1"), Some(&SmolStr::from("value1")));
        assert_eq!(merged.get("key2"), Some(&SmolStr::from("value2")));
    }
}
