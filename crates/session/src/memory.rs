//! Memory abstraction module.

#![allow(clippy::pedantic)]

pub mod file_backend;

pub use file_backend::FileMemory;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use neoco_core::ids::SessionUlid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Memory trait for storing and retrieving memory entries.
#[async_trait]
pub trait Memory: Send + Sync {
    /// Stores a memory entry.
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError>;
    /// Recalls memory entries matching a query.
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError>;
    /// Gets a memory entry by key.
    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>, MemoryError>;
    /// Deletes a memory entry by key.
    async fn delete(&self, key: &str) -> Result<(), MemoryError>;
    /// Clears all memory entries.
    async fn clear(&self) -> Result<(), MemoryError>;
}

/// Memory entry containing key, content, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Entry key.
    pub key: String,
    /// Entry content.
    pub content: String,
    /// Entry category.
    pub category: MemoryCategory,
    /// Importance level (0.0 to 1.0).
    pub importance: f32,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl MemoryEntry {
    /// Creates a new memory entry.
    pub fn new(
        key: impl Into<String>,
        content: impl Into<String>,
        category: MemoryCategory,
    ) -> Self {
        Self {
            key: key.into(),
            content: content.into(),
            category,
            importance: 0.5,
            created_at: Utc::now(),
        }
    }

    /// Sets the importance level.
    #[must_use]
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance;
        self
    }
}

/// Memory category for organizing entries.
#[derive(Debug, Clone)]
pub enum MemoryCategory {
    /// Global memory (available everywhere).
    Global,
    /// Directory-scoped memory.
    Directory(PathBuf),
    /// Session-scoped memory.
    Session(SessionUlid),
}

impl Serialize for MemoryCategory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            MemoryCategory::Global => serializer.serialize_str("global"),
            MemoryCategory::Directory(path) => {
                serializer.serialize_str(&format!("dir:{}", path.display()))
            },
            MemoryCategory::Session(sid) => serializer.serialize_str(&format!("session:{sid}")),
        }
    }
}

impl<'de> Deserialize<'de> for MemoryCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s == "global" {
            Ok(MemoryCategory::Global)
        } else if let Some(path_str) = s.strip_prefix("dir:") {
            Ok(MemoryCategory::Directory(PathBuf::from(path_str)))
        } else if let Some(sid_str) = s.strip_prefix("session:") {
            let sid = SessionUlid::from_string(sid_str).map_err(serde::de::Error::custom)?;
            Ok(MemoryCategory::Session(sid))
        } else {
            Err(serde::de::Error::custom("invalid memory category"))
        }
    }
}

/// Memory errors.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// Store failed.
    #[error("存储失败: {0}")]
    StoreFailed(String),
    /// Recall failed.
    #[error("检索失败: {0}")]
    RecallFailed(String),
    /// Entry not found.
    #[error("不存在: {0}")]
    NotFound(String),
}

/// In-memory memory storage.
pub struct InMemoryMemory {
    /// Memory entries.
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl InMemoryMemory {
    /// Creates a new InMemoryMemory.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        let mut entries = self.entries.write().await;
        entries.insert(entry.key.clone(), entry);
        Ok(())
    }

    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError> {
        let entries = self.entries.read().await;
        let query_lower = query.to_lowercase();

        let mut results: Vec<_> = entries
            .values()
            .filter(|e| e.content.to_lowercase().contains(&query_lower))
            .cloned()
            .collect();

        results.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
        results.truncate(limit);

        Ok(results)
    }

    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let entries = self.entries.read().await;
        Ok(entries.get(key).cloned())
    }

    async fn delete(&self, key: &str) -> Result<(), MemoryError> {
        let mut entries = self.entries.write().await;
        entries.remove(key);
        Ok(())
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        let mut entries = self.entries.write().await;
        entries.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_memory_entry_creation() {
        let entry = MemoryEntry::new("test_key", "Test content", MemoryCategory::Global);

        assert_eq!(entry.key, "test_key");
        assert_eq!(entry.content, "Test content");
        assert!(matches!(entry.category, MemoryCategory::Global));
    }

    #[test]
    fn test_memory_entry_with_importance() {
        let entry = MemoryEntry::new("key", "content", MemoryCategory::Global).with_importance(0.9);

        assert!((entry.importance - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_memory_category_serialization() {
        let global = MemoryCategory::Global;
        let serialized = serde_json::to_string(&global).unwrap();
        assert_eq!(serialized, "\"global\"");

        let deserialized: MemoryCategory = serde_json::from_str(&serialized).unwrap();
        assert!(matches!(deserialized, MemoryCategory::Global));
    }

    #[test]
    fn test_memory_category_directory_serialization() {
        let dir = MemoryCategory::Directory(PathBuf::from("/test/path"));
        let serialized = serde_json::to_string(&dir).unwrap();

        let deserialized: MemoryCategory = serde_json::from_str(&serialized).unwrap();
        assert!(
            matches!(deserialized, MemoryCategory::Directory(p) if p == Path::new("/test/path"))
        );
    }

    #[test]
    fn test_memory_category_session_serialization() {
        let session = SessionUlid::new();
        let cat = MemoryCategory::Session(session);
        let serialized = serde_json::to_string(&cat).unwrap();

        let deserialized: MemoryCategory = serde_json::from_str(&serialized).unwrap();
        assert!(matches!(deserialized, MemoryCategory::Session(s) if s == session));
    }
}
