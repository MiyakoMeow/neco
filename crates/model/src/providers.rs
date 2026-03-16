//! Provider module

pub mod adapter;
pub mod anthropic;
pub mod openai;

#[cfg(test)]
mod tests;

pub use adapter::{build_client, build_multi_key_client, build_openai_client, resolve_api_keys};
pub use anthropic::AnthropicClient;
pub use openai::OpenAiClient;
