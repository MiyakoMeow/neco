//! Configuration validation module

use crate::{Config, ConfigError, SmolStr};

/// Configuration validator
///
/// Provides configuration validation functionality.
pub struct ConfigValidator;

impl ConfigValidator {
    /// Validate the entire configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if any validation check fails.
    pub fn validate(config: &Config) -> Result<(), ConfigError> {
        Self::validate_model(config)?;
        Self::validate_model_groups(config)?;
        Self::validate_providers(config)?;
        Self::validate_mcp_servers(config)?;
        Ok(())
    }

    /// Validate the model field.
    ///
    /// # Errors
    ///
    /// Returns an error if the model references a non-existent provider.
    pub fn validate_model(config: &Config) -> Result<(), ConfigError> {
        if let Some(ref model) = config.model
            && let Some((provider, _model_name)) = model.split_once('/')
            && !config.model_providers.contains_key(provider)
        {
            return Err(ConfigError::ValidationError(format!(
                "model '{model}' 引用的 provider '{provider}' 不存在"
            )));
        }

        Ok(())
    }

    /// Validate model groups.
    ///
    /// # Errors
    ///
    /// Returns an error if a model group has no models or references a non-existent provider.
    pub fn validate_model_groups(config: &Config) -> Result<(), ConfigError> {
        if config.model_groups.is_empty() {
            return Ok(());
        }

        for (group_name, group) in config.model_groups.iter() {
            if group.models.is_empty() {
                return Err(ConfigError::ValidationError(format!(
                    "模型组 '{group_name}' 没有模型"
                )));
            }

            for model_ref in &group.models {
                if !config
                    .model_providers
                    .contains_key(model_ref.provider.as_str())
                {
                    return Err(ConfigError::ValidationError(format!(
                        "模型组 '{group_name}' 引用的 provider '{}' 不存在",
                        model_ref.provider
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate model providers.
    ///
    /// # Errors
    ///
    /// Returns an error if a provider has an invalid URL, zero timeout, or invalid API key.
    pub fn validate_providers(config: &Config) -> Result<(), ConfigError> {
        if config.model_providers.is_empty() {
            return Ok(());
        }

        for (provider_name, provider) in config.model_providers.iter() {
            if provider.base_url.host_str().is_none() {
                return Err(ConfigError::InvalidUrl(format!(
                    "provider '{provider_name}' 的 base_url 无效"
                )));
            }

            if provider.timeout.is_zero() {
                return Err(ConfigError::ValidationError(format!(
                    "provider '{provider_name}' 的 timeout 不能为零"
                )));
            }

            if let Err(e) = Self::validate_api_key(provider_name.as_str(), provider) {
                return Err(ConfigError::ValidationError(format!(
                    "provider '{provider_name}' API 密钥验证失败: {e}",
                )));
            }
        }

        Ok(())
    }

    /// Validate API key configuration for a provider.
    fn validate_api_key(
        provider_name: &str,
        provider: &crate::ModelProvider,
    ) -> Result<(), ConfigError> {
        if let Some(ref env_name) = provider.api_key_env {
            return std::env::var(env_name.as_str())
                .map(|_| ())
                .map_err(|_| ConfigError::EnvVarNotFound(env_name.to_string()));
        }

        if let Some(ref env_names) = provider.api_key_envs {
            for name in env_names {
                if std::env::var(name.as_str()).is_ok() {
                    return Ok(());
                }
            }
            let names: Vec<&str> = env_names.iter().map(SmolStr::as_str).collect();
            return Err(ConfigError::ValidationError(format!(
                "provider '{provider_name}' 的环境变量 {} 均未设置",
                names.join(", ")
            )));
        }

        if provider.api_key.is_some() {
            return Ok(());
        }

        Err(ConfigError::ValidationError(format!(
            "provider '{provider_name}' 没有配置 API 密钥",
        )))
    }

    /// Validate MCP servers.
    ///
    /// # Errors
    ///
    /// Returns an error if an MCP server has no valid transport, command, or URL configuration.
    pub fn validate_mcp_servers(config: &Config) -> Result<(), ConfigError> {
        if config.mcp_servers.is_empty() {
            return Ok(());
        }

        for (server_name, server) in config.mcp_servers.iter() {
            let has_transport = server.transport.is_some();
            let has_command = server.command.is_some();
            let has_url = server.url.is_some();

            if !has_transport && !has_command && !has_url {
                return Err(ConfigError::ValidationError(format!(
                    "MCP服务器 '{server_name}' 没有配置 transport、command 或 url"
                )));
            }

            if let Some(transport) = &server.transport {
                match transport.transport_type {
                    crate::McpTransportType::Stdio => {
                        if transport.command.is_none() {
                            return Err(ConfigError::ValidationError(format!(
                                "MCP服务器 '{server_name}' 使用 stdio transport 但没有配置 command"
                            )));
                        }
                    },
                    crate::McpTransportType::Http => {
                        if transport.url.is_none() {
                            return Err(ConfigError::ValidationError(format!(
                                "MCP服务器 '{server_name}' 使用 http transport 但没有配置 url"
                            )));
                        }
                    },
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IndexMap, McpServerConfig, McpServers, ModelGroup, ModelGroups, ModelProvider,
        ModelProviders, ModelRef, ProviderType, SystemConfig,
    };
    use chrono::Duration;
    use url::Url;

    fn create_valid_config() -> Config {
        let mut model_groups = ModelGroups::new();
        model_groups.insert(
            SmolStr::from("test"),
            ModelGroup {
                models: vec![ModelRef {
                    provider: SmolStr::from("openai"),
                    name: SmolStr::from("gpt-4"),
                    params: IndexMap::new(),
                }],
                ..Default::default()
            },
        );

        let mut model_providers = ModelProviders::new();
        model_providers.insert(
            SmolStr::from("openai"),
            ModelProvider {
                provider_type: ProviderType::OpenAI,
                name: Some(SmolStr::from("OpenAI")),
                base_url: Url::parse("https://api.openai.com/v1").unwrap(),
                api_key_env: None,
                api_key_envs: None,
                api_key: Some(crate::SecretString::new("test-api-key")),
                default_params: IndexMap::new(),
                retry: crate::RetryConfig::default(),
                timeout: Duration::seconds(60),
            },
        );

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
    fn test_validate_valid_config() {
        let config = create_valid_config();
        ConfigValidator::validate(&config).unwrap();
    }

    #[test]
    fn test_validate_empty_model_group() {
        let mut config = create_valid_config();
        config.model_groups.insert(
            SmolStr::from("empty"),
            ModelGroup {
                models: vec![],
                ..Default::default()
            },
        );

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_missing_provider() {
        let mut config = create_valid_config();
        config.model_groups.insert(
            SmolStr::from("missing"),
            ModelGroup {
                models: vec![ModelRef {
                    provider: SmolStr::from("nonexistent"),
                    name: SmolStr::from("model"),
                    params: IndexMap::new(),
                }],
                ..Default::default()
            },
        );

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_base_url() {
        let mut config = create_valid_config();
        config.model_providers.insert(
            SmolStr::from("valid_but_no_host"),
            ModelProvider {
                provider_type: ProviderType::OpenAI,
                name: Some(SmolStr::from("Valid No Host")),
                base_url: Url::parse("http://localhost").unwrap(),
                api_key_env: None,
                api_key_envs: None,
                api_key: Some(crate::SecretString::new("test-api-key")),
                default_params: IndexMap::new(),
                retry: crate::RetryConfig::default(),
                timeout: Duration::seconds(60),
            },
        );

        let result = ConfigValidator::validate(&config);
        if let Err(e) = result {
            panic!("Valid URL with host should pass: {e:?}");
        }
    }

    #[test]
    fn test_validate_zero_timeout() {
        let mut config = create_valid_config();
        config.model_providers.insert(
            SmolStr::from("zero-timeout"),
            ModelProvider {
                provider_type: ProviderType::OpenAI,
                name: Some(SmolStr::from("Zero Timeout")),
                base_url: Url::parse("https://api.example.com").unwrap(),
                api_key_env: None,
                api_key_envs: None,
                api_key: None,
                default_params: IndexMap::new(),
                retry: crate::RetryConfig::default(),
                timeout: Duration::zero(),
            },
        );

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mcp_server_no_config() {
        let mut config = create_valid_config();
        config.mcp_servers.insert(
            SmolStr::from("empty"),
            McpServerConfig {
                command: None,
                args: None,
                url: None,
                bearer_token_env: None,
                http_headers: IndexMap::new(),
                env: IndexMap::new(),
                transport: None,
            },
        );

        let result = ConfigValidator::validate(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mcp_server_with_command() {
        let mut config = create_valid_config();
        config.mcp_servers.insert(
            SmolStr::from("valid"),
            McpServerConfig {
                command: Some(SmolStr::from("npx")),
                args: Some(vec![SmolStr::from("-y"), SmolStr::from("@some/package")]),
                url: None,
                bearer_token_env: None,
                http_headers: IndexMap::new(),
                env: IndexMap::new(),
                transport: None,
            },
        );

        ConfigValidator::validate(&config).unwrap();
    }
}
