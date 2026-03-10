//! File-based memory backend.

#![allow(clippy::pedantic)]

use crate::memory::{Memory, MemoryEntry, MemoryError};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// File-based memory storage.
pub struct FileMemory {
    /// Base path for storage.
    base_path: PathBuf,
}

impl FileMemory {
    /// Creates a new FileMemory.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Gets the entry path for a key.
    fn entry_path(&self, key: &str) -> PathBuf {
        let safe_key = key.replace(['/', '\\', ':'], "_");
        self.base_path.join(format!("{}.json", safe_key))
    }

    /// Ensures the base directory exists.
    async fn ensure_base_dir(&self) -> Result<(), MemoryError> {
        if !self.base_path.exists() {
            fs::create_dir_all(&self.base_path)
                .await
                .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;
        }
        Ok(())
    }
}

#[async_trait]
impl Memory for FileMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.ensure_base_dir().await?;

        let path = self.entry_path(&entry.key);
        let content = serde_json::to_string_pretty(&entry)
            .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;

        let mut file = fs::File::create(&path)
            .await
            .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;

        file.write_all(content.as_bytes())
            .await
            .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;

        Ok(())
    }

    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }

        let mut dir = fs::read_dir(&self.base_path)
            .await
            .map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| MemoryError::RecallFailed(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let mut file = fs::File::open(&path)
                    .await
                    .map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

                let mut content = String::new();
                file.read_to_string(&mut content)
                    .await
                    .map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

                if let Ok(mem_entry) = serde_json::from_str::<MemoryEntry>(&content)
                    && mem_entry.content.to_lowercase().contains(&query_lower)
                {
                    results.push(mem_entry);
                }
            }
        }

        results.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
        results.truncate(limit);

        Ok(results)
    }

    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let path = self.entry_path(key);

        if !path.exists() {
            return Ok(None);
        }

        let mut file = fs::File::open(&path)
            .await
            .map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

        let entry: MemoryEntry =
            serde_json::from_str(&content).map_err(|e| MemoryError::RecallFailed(e.to_string()))?;

        Ok(Some(entry))
    }

    async fn delete(&self, key: &str) -> Result<(), MemoryError> {
        let path = self.entry_path(key);

        if path.exists() {
            fs::remove_file(&path)
                .await
                .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;
        }

        Ok(())
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        if !self.base_path.exists() {
            return Ok(());
        }

        let mut dir = fs::read_dir(&self.base_path)
            .await
            .map_err(|e| MemoryError::StoreFailed(e.to_string()))?;

        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| MemoryError::StoreFailed(e.to_string()))?
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let _ = fs::remove_file(&path).await;
            }
        }

        Ok(())
    }
}
