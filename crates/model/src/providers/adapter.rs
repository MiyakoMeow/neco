//! Model provider adapter module

use crate::client::{ModelClient, MultiKeyClient};
use crate::error::ModelError;
use crate::providers::{AnthropicClient, OpenAiClient};
use crate::types::RetryConfig;
use neoco_config::{ModelProvider, ProviderType};
use std::sync::Arc;

/// Build a model client from a ModelProvider configuration
///
/// # Errors
///
/// Returns an error if the provider type is not supported or if the API key is invalid
pub fn build_client(
    provider: &ModelProvider,
    provider_key: &str,
    model_name: Option<&str>,
) -> Result<Box<dyn crate::client::ModelClient>, ModelError> {
    let api_key = resolve_api_key(provider)?;
    let model_name = model_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| provider.name_or(provider_key).to_string());
    let base_url = provider.base_url.to_string();

    match provider.provider_type {
        ProviderType::OpenAI => {
            let client = OpenAiClient::new(api_key.as_str(), &model_name)
                .map_err(|e| ModelError::Config(e.to_string()))?
                .with_base_url(&base_url);
            Ok(Box::new(client))
        },
        ProviderType::Anthropic => {
            let client = AnthropicClient::new(api_key.as_str(), &model_name)
                .map_err(|e| ModelError::Config(e.to_string()))?
                .with_base_url(&base_url);
            Ok(Box::new(client))
        },
        ProviderType::OpenRouter => {
            let client = OpenAiClient::new(api_key.as_str(), &model_name)
                .map_err(|e| ModelError::Config(e.to_string()))?
                .with_base_url(&base_url)
                .with_provider("openrouter");
            Ok(Box::new(client))
        },
    }
}

/// Build an OpenAiClient from a ModelProvider configuration
///
/// # Errors
///
/// Returns an error if the API key cannot be resolved
pub fn build_openai_client(
    provider: &ModelProvider,
    provider_key: &str,
) -> Result<OpenAiClient, ModelError> {
    let api_key = resolve_api_key(provider)?;
    let base_url = provider.base_url.to_string();

    let client = OpenAiClient::new(api_key.as_str(), provider.name_or(provider_key).as_ref())
        .map_err(|e| ModelError::Config(e.to_string()))?
        .with_base_url(&base_url);

    Ok(client)
}

/// Resolves a single API key from provider configuration.
fn resolve_api_key(provider: &ModelProvider) -> Result<String, ModelError> {
    if let Some(ref env_name) = provider.api_key_env {
        return std::env::var(env_name.as_str())
            .map_err(|_| ModelError::Config(format!("Environment variable {env_name} not found")));
    }

    if let Some(ref env_names) = provider.api_key_envs {
        for name in env_names {
            if let Ok(value) = std::env::var(name.as_str()) {
                return Ok(value);
            }
        }
        return Err(ModelError::Config(format!(
            "None of the environment variables found: {env_names:?}",
        )));
    }

    if let Some(ref api_key) = provider.api_key {
        return Ok(api_key.as_str().to_string());
    }

    Err(ModelError::Config("No API key configured".to_string()))
}

/// Resolves API keys from environment variables.
pub fn resolve_api_keys(provider: &ModelProvider) -> Result<Vec<String>, ModelError> {
    if let Some(ref env_name) = provider.api_key_env {
        return std::env::var(env_name.as_str())
            .map(|v| vec![v])
            .map_err(|_| ModelError::Config(format!("Environment variable {env_name} not found")));
    }

    if let Some(ref env_names) = provider.api_key_envs {
        let mut keys = Vec::new();
        for name in env_names {
            if let Ok(value) = std::env::var(name.as_str()) {
                keys.push(value);
            }
        }
        if keys.is_empty() {
            return Err(ModelError::Config(format!(
                "None of the environment variables found: {env_names:?}",
            )));
        }
        return Ok(keys);
    }

    if let Some(ref api_key) = provider.api_key {
        return Ok(vec![api_key.as_str().to_string()]);
    }

    Err(ModelError::Config("No API key configured".to_string()))
}

/// Build a MultiKeyClient that automatically rotates through multiple API keys
///
/// # Errors
///
/// Returns an error if the provider type is not supported or if no API keys are available
pub fn build_multi_key_client(
    provider: &ModelProvider,
    provider_key: &str,
    retry_config: RetryConfig,
    model_name: Option<&str>,
) -> Result<MultiKeyClient, ModelError> {
    let api_keys = resolve_api_keys(provider)?;
    let base_url = provider.base_url.to_string();
    let provider_type = provider.provider_type;
    let model_name = model_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| provider.name_or(provider_key).to_string());

    let clients: Vec<Arc<dyn ModelClient>> = match provider_type {
        ProviderType::OpenAI | ProviderType::OpenRouter => {
            let mut result_clients = Vec::new();
            for key in &api_keys {
                let client = OpenAiClient::new(key.as_str(), model_name.as_str())
                    .map_err(|e| ModelError::Config(e.to_string()))?
                    .with_base_url(&base_url);
                result_clients.push(Arc::new(client) as Arc<dyn ModelClient>);
            }
            result_clients
        },
        ProviderType::Anthropic => {
            let mut result_clients = Vec::new();
            for key in &api_keys {
                let client = AnthropicClient::new(key.as_str(), model_name.as_str())
                    .map_err(|e| ModelError::Config(e.to_string()))?
                    .with_base_url(&base_url);
                result_clients.push(Arc::new(client) as Arc<dyn ModelClient>);
            }
            result_clients
        },
    };

    if clients.is_empty() {
        return Err(ModelError::Config("No valid API keys found".to_string()));
    }

    Ok(MultiKeyClient::new(clients, retry_config))
}
