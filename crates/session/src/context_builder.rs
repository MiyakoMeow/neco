//! Context builder module.

#![allow(dead_code)]
#![allow(clippy::pedantic)]

use crate::agent::Agent;
use crate::message::Message;
use neoco_core::messages::Role;

/// Token counter trait for estimating token usage.
pub trait TokenCounter: Send + Sync {
    /// Estimates the number of tokens in a string.
    fn estimate_string_tokens(&self, text: &str) -> usize;
    /// Estimates the number of tokens in a message.
    fn estimate_message_tokens(&self, message: &Message) -> usize;
}

/// Simple token counter implementation.
pub struct SimpleTokenCounter;

impl SimpleTokenCounter {
    /// Creates a new SimpleTokenCounter.
    pub fn new() -> Self {
        Self
    }

    /// Calculates role overhead.
    fn role_overhead(role: Role) -> usize {
        match role {
            Role::System => 4,
            Role::User | Role::Assistant | Role::Tool => 5,
        }
    }
}

impl Default for SimpleTokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for SimpleTokenCounter {
    fn estimate_string_tokens(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        ((char_count as f64 / 4.0).ceil() as usize).max(1)
    }

    fn estimate_message_tokens(&self, message: &Message) -> usize {
        let mut tokens = Self::role_overhead(message.role);
        tokens += self.estimate_string_tokens(&message.content);
        if let Some(ref tool_calls) = message.tool_calls {
            tokens += 12;
            for tc in tool_calls {
                tokens += self.estimate_string_tokens(&tc.id);
                tokens += self.estimate_string_tokens(&tc.tool_name.to_string());
                tokens += self.estimate_string_tokens(&tc.arguments);
            }
        }
        if message.tool_call_id.is_some() {
            tokens += 3;
        }
        tokens += 3;
        tokens
    }
}

/// Context message for building conversation context.
#[derive(Clone, Debug)]
pub struct ContextMessage {
    /// The role of the message.
    pub role: Role,
    /// The content of the message.
    pub content: String,
}

impl ContextMessage {
    /// Creates a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    /// Creates a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    /// Creates an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }

    /// Creates a tool message.
    pub fn tool(content: impl Into<String>, _tool_call_id: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
        }
    }

    /// Creates a ContextMessage from a Message.
    pub fn from_message(msg: &Message) -> Self {
        Self {
            role: msg.role,
            content: msg.content.clone(),
        }
    }
}

/// Builder for constructing conversation context.
pub struct ContextBuilder<'a, T: TokenCounter> {
    /// System messages.
    system_messages: Vec<String>,
    /// Conversation messages.
    conversation: Vec<ContextMessage>,
    /// Active tools.
    active_tools: Vec<String>,
    /// Maximum tokens.
    max_tokens: Option<usize>,
    /// Token counter.
    token_counter: Option<&'a T>,
}

impl<'a, T: TokenCounter> ContextBuilder<'a, T> {
    /// Creates a new ContextBuilder.
    pub fn new() -> Self {
        Self {
            system_messages: Vec::new(),
            conversation: Vec::new(),
            active_tools: Vec::new(),
            max_tokens: None,
            token_counter: None,
        }
    }

    /// Sets the token counter.
    pub fn with_token_counter(&mut self, counter: &'a T) -> &mut Self {
        self.token_counter = Some(counter);
        self
    }

    /// Sets the maximum token limit.
    pub fn with_max_tokens(&mut self, max_tokens: usize) -> &mut Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Adds a system message.
    pub fn with_system_message(&mut self, message: impl Into<String>) -> &mut Self {
        self.system_messages.push(message.into());
        self
    }

    /// Adds multiple system messages.
    pub fn with_system_messages(&mut self, messages: Vec<String>) -> &mut Self {
        self.system_messages.extend(messages);
        self
    }

    /// Adds messages from an agent.
    pub fn with_agent_messages(&mut self, agent: &'a Agent) -> &mut Self {
        for msg in &agent.messages {
            self.conversation.push(ContextMessage::from_message(msg));
        }
        self
    }

    /// Sets the active tools.
    pub fn with_tools(&mut self, tools: Vec<String>) -> &mut Self {
        self.active_tools = tools;
        self
    }

    /// Builds the context result.
    pub fn build(&self) -> Result<ContextBuildResult, ContextBuildError> {
        let mut messages = Vec::new();

        if !self.system_messages.is_empty() {
            let system_content = self.system_messages.join("\n\n");
            messages.push(ContextMessage::system(system_content));
        }

        for msg in &self.conversation {
            messages.push(msg.clone());
        }

        if let (Some(max_tokens), Some(counter)) = (self.max_tokens, self.token_counter) {
            let current_tokens: usize = messages
                .iter()
                .map(|m| counter.estimate_string_tokens(&m.content))
                .sum();

            if current_tokens > max_tokens {
                let mut truncated = Vec::new();
                let mut current = 0;

                for msg in messages.iter().rev() {
                    let msg_tokens = counter.estimate_string_tokens(&msg.content);
                    if current + msg_tokens > max_tokens {
                        break;
                    }
                    current += msg_tokens;
                    truncated.push(msg.clone());
                }

                truncated.reverse();
                messages = truncated;
            }
        }

        Ok(ContextBuildResult {
            messages,
            tools: self.active_tools.clone(),
        })
    }
}

impl<T: TokenCounter> Default for ContextBuilder<'_, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of building the context.
#[derive(Clone, Debug)]
pub struct ContextBuildResult {
    /// The messages in the context.
    pub messages: Vec<ContextMessage>,
    /// The active tools.
    pub tools: Vec<String>,
}

/// Error that can occur when building context.
#[derive(Debug, thiserror::Error)]
pub enum ContextBuildError {
    /// Token calculation error.
    #[error("Token计算错误: {0}")]
    TokenCalculation(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentModeParsed;

    #[test]
    fn test_context_builder_basic() {
        let agent = Agent::new(
            neoco_core::ids::AgentUlid::new_root(&neoco_core::ids::SessionUlid::new()),
            None,
            Some("test".to_string()),
            AgentModeParsed::Primary,
            None,
            None,
        );

        let counter = SimpleTokenCounter::new();
        let mut builder = ContextBuilder::new();
        builder
            .with_token_counter(&counter)
            .with_system_message("You are a helpful assistant.")
            .with_agent_messages(&agent)
            .with_max_tokens(10000);

        let result = builder.build().unwrap();
        assert!(!result.messages.is_empty());
    }

    #[test]
    fn test_token_counter() {
        let counter = SimpleTokenCounter::new();
        let tokens = counter.estimate_string_tokens("Hello world");
        assert!(tokens > 0);
    }
}
