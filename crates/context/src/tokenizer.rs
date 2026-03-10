//! Token counting module.

use neoco_core::messages::{Message, Role, ToolCall};

/// Token counter trait.
pub trait TokenCounter: Send + Sync {
    /// Estimates tokens in a string.
    fn estimate_string_tokens(&self, text: &str) -> usize;
    /// Estimates tokens in messages.
    fn estimate_tokens(&self, messages: &[Message]) -> usize;
    /// Estimates tokens in a message.
    fn estimate_message_tokens(&self, message: &Message) -> usize;
}

/// Simple token counter implementation.
pub struct SimpleCounter;

impl SimpleCounter {
    /// Creates a new `SimpleCounter`.
    pub fn new() -> Self {
        Self
    }

    /// Returns token overhead for a role.
    fn role_overhead(role: Role) -> usize {
        match role {
            Role::System => 4,
            Role::User | Role::Assistant | Role::Tool => 5,
        }
    }

    /// Estimates tokens for tool calls.
    fn tool_calls_tokens(&self, tool_calls: &[ToolCall]) -> usize {
        let mut tokens = 0;
        for tc in tool_calls {
            tokens += 12;
            tokens += self.estimate_string_tokens(&tc.id);
            tokens += self.estimate_string_tokens(tc.tool_name.as_str());
            tokens += self.estimate_string_tokens(&tc.arguments);
        }
        tokens
    }
}

impl Default for SimpleCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for SimpleCounter {
    fn estimate_string_tokens(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        #[allow(clippy::cast_precision_loss)]
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let tokens = (char_count as f64 / 4.0).ceil() as usize;
        tokens
    }

    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| self.estimate_message_tokens(m))
            .sum()
    }

    fn estimate_message_tokens(&self, message: &Message) -> usize {
        let mut tokens = Self::role_overhead(message.role);
        tokens += self.estimate_string_tokens(&message.content);
        if let Some(ref tool_calls) = message.tool_calls {
            tokens += self.tool_calls_tokens(tool_calls);
            tokens += 12;
        }
        if message.tool_call_id.is_some() {
            tokens += 3;
        }
        tokens += 3;
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenSavings;

    #[test]
    fn test_estimate_string_tokens() {
        let counter = SimpleCounter::new();
        assert_eq!(counter.estimate_string_tokens("hello"), 2);
        assert_eq!(counter.estimate_string_tokens(""), 0);
    }

    #[test]
    fn test_estimate_message_tokens() {
        let counter = SimpleCounter::new();
        let msg = Message::user("Hello, world!");
        let tokens = counter.estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[test]
    fn test_token_savings_calculation() {
        let savings = TokenSavings::new(1000, 300);
        assert_eq!(savings.saved, 700);
        assert!((savings.saved_percent - 70.0).abs() < f64::EPSILON);
    }
}
