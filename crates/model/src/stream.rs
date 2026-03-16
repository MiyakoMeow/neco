//! Streaming output handling

use crate::client::BoxStream;
use crate::error::ModelError;
use crate::types::{ChatChunk, ChatResponse, Choice, Message, ToolCall, Usage};
use futures::StreamExt;
use std::collections::HashMap;

/// Handler for streaming responses.
pub struct StreamHandler;

impl StreamHandler {
    /// Collects all chunks into a single string.
    pub async fn collect_full_response(
        mut stream: BoxStream<Result<ChatChunk, ModelError>>,
    ) -> Result<String, ModelError> {
        let mut content = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            if let Some(choice) = chunk.choices.first() {
                if let Some(ref c) = choice.delta.content {
                    content.push_str(c);
                }
            }
        }

        Ok(content)
    }

    /// Processes the stream with a callback function.
    pub async fn process_with_callback<F>(
        mut stream: BoxStream<Result<ChatChunk, ModelError>>,
        mut callback: F,
    ) -> Result<ChatResponse, ModelError>
    where
        F: FnMut(&str),
    {
        let mut full_content = String::new();
        let mut final_id = String::new();
        let mut tool_calls_map: HashMap<String, ToolCall> = HashMap::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            if let Some(choice) = chunk.choices.first() {
                if let Some(ref content) = choice.delta.content {
                    full_content.push_str(content);
                    callback(content);
                }
                if let Some(tc_list) = &choice.delta.tool_calls {
                    for tc in tc_list {
                        if let Some(existing) = tool_calls_map.get_mut(&tc.id) {
                            if !tc.name.is_empty() {
                                existing.name = tc.name.clone();
                            }
                            if tc.arguments != serde_json::Value::Null {
                                let existing_args = existing.arguments.as_object_mut();
                                let new_args = tc.arguments.as_object();
                                if let (Some(em), Some(nm)) = (existing_args, new_args) {
                                    for (k, v) in nm {
                                        em.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                        } else {
                            tool_calls_map.insert(
                                tc.id.clone(),
                                ToolCall {
                                    id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    arguments: tc.arguments.clone(),
                                },
                            );
                        }
                    }
                }
                if choice.index == 0 {
                    final_id = chunk.id.clone();
                }
            }
        }

        let tool_calls = if tool_calls_map.is_empty() {
            None
        } else {
            let mut calls: Vec<ToolCall> = tool_calls_map.into_values().collect();
            calls.sort_by(|a, b| a.id.cmp(&b.id));
            Some(calls)
        };

        Ok(ChatResponse {
            id: final_id,
            model: String::new(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: full_content,
                    tool_calls,
                    tool_call_id: None,
                },
                finish_reason: None,
            }],
            usage: Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
        })
    }
}
