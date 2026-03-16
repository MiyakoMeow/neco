//! Tests for OpenAI-compatible providers (e.g., ZhipuAI)

#[cfg(test)]
mod openai_provider_tests {
    use crate::client::ModelClient;
    use crate::providers::{OpenAiClient, build_client};
    use indexmap::IndexMap;
    use neoco_config::{Duration, ModelProvider, ProviderType, RetryConfig, SecretString};
    use smol_str::SmolStr;

    const TEST_BASE_URL: &str = "https://open.bigmodel.cn/api/coding/paas/v4";
    const TEST_MODEL: &str = "zhipuai-coding-plan/glm-4.7";
    const TEST_API_KEY: &str = "test-api-key-for-testing";

    #[test]
    fn test_openai_client_with_custom_base_url() {
        let client = OpenAiClient::new(TEST_API_KEY, TEST_MODEL)
            .expect("Failed to create OpenAiClient")
            .with_base_url(TEST_BASE_URL)
            .with_provider("zhipuai");

        assert_eq!(client.model_name(), TEST_MODEL);
        assert_eq!(client.provider_name(), "zhipuai");
    }

    #[test]
    fn test_zhipuai_coding_plan_config() {
        let client = OpenAiClient::new("fake-key", "zhipuai-coding-plan/glm-4.7")
            .expect("Failed to create OpenAiClient")
            .with_base_url("https://open.bigmodel.cn/api/coding/paas/v4");

        assert_eq!(client.model_name(), "zhipuai-coding-plan/glm-4.7");
    }

    #[test]
    fn test_openai_client_requires_api_key() {
        let result = OpenAiClient::new("", "test-model");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_openai_client_stream_not_actual_api_call() {
        let client = OpenAiClient::new(TEST_API_KEY, TEST_MODEL)
            .expect("Failed to create OpenAiClient")
            .with_base_url(TEST_BASE_URL);

        let _capabilities = client.capabilities();
        assert_eq!(client.provider_name(), "openai");
    }

    #[test]
    fn test_build_client_from_config() {
        let provider_config = ModelProvider {
            provider_type: ProviderType::OpenAI,
            name: Some(SmolStr::from("zhipuai")),
            base_url: TEST_BASE_URL.parse().expect("Invalid URL"),
            api_key_env: None,
            api_key_envs: None,
            api_key: Some(SecretString::new(TEST_API_KEY)),
            default_params: IndexMap::new(),
            retry: RetryConfig::default(),
            timeout: Duration::default(),
        };

        let client = build_client(&provider_config, "zhipuai", None)
            .expect("Failed to build client from config");

        assert_eq!(client.model_name(), "zhipuai");
        assert_eq!(client.provider_name(), "openai");
    }

    #[test]
    fn test_build_client_with_env_api_key() {
        unsafe {
            std::env::set_var("TEST_API_KEY", "env-api-key-value");
        }

        let provider_config = ModelProvider {
            provider_type: ProviderType::OpenAI,
            name: Some(SmolStr::from("test-provider")),
            base_url: "https://api.example.com/v1".parse().expect("Invalid URL"),
            api_key_env: Some(SmolStr::from("TEST_API_KEY")),
            api_key_envs: None,
            api_key: None,
            default_params: IndexMap::new(),
            retry: RetryConfig::default(),
            timeout: Duration::default(),
        };

        let client = build_client(&provider_config, "test-provider", None)
            .expect("Failed to build client with env API key");

        assert_eq!(client.model_name(), "test-provider");
        assert_eq!(client.provider_name(), "openai");

        unsafe {
            std::env::remove_var("TEST_API_KEY");
        }
    }

    #[test]
    fn test_build_client_openrouter() {
        let provider_config = ModelProvider {
            provider_type: ProviderType::OpenRouter,
            name: Some(SmolStr::from("openrouter")),
            base_url: "https://openrouter.ai/api/v1".parse().expect("Invalid URL"),
            api_key_env: None,
            api_key_envs: None,
            api_key: Some(SecretString::new("or-test-key")),
            default_params: IndexMap::new(),
            retry: RetryConfig::default(),
            timeout: Duration::default(),
        };

        let client = build_client(&provider_config, "openrouter", None)
            .expect("Failed to build OpenRouter client");

        assert_eq!(client.model_name(), "openrouter");
        assert_eq!(client.provider_name(), "openrouter");
    }
}
