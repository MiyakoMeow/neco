//! File system storage implementation.

#![allow(clippy::pedantic)]

use crate::agent::{Agent, HierarchyMeta};
use crate::message::Message;
use crate::session::Session;
use crate::session::SessionMeta;
use crate::storage::{StorageBackend, StorageError};
use async_trait::async_trait;
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// File-based storage.
pub struct FileStorage {
    /// Base path for storage.
    base_path: PathBuf,
}

impl FileStorage {
    /// Creates a new FileStorage.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// Gets the session directory.
    fn session_dir(&self, session_id: &SessionUlid) -> PathBuf {
        self.base_path.join(session_id.to_string())
    }

    /// Gets the session file path.
    fn session_file(&self, session_id: &SessionUlid) -> PathBuf {
        self.session_dir(session_id).join("session.toml")
    }

    /// Gets the hierarchy file path.
    /// Gets the hierarchy file path.
    fn hierarchy_file(&self, session_id: &SessionUlid) -> PathBuf {
        self.session_dir(session_id).join("hierarchy.toml")
    }

    /// Gets the agents directory.
    fn agents_dir(&self, session_id: &SessionUlid) -> PathBuf {
        self.session_dir(session_id).join("agents")
    }

    /// Gets the agent file path.
    fn agent_file(&self, agent_id: &AgentUlid) -> PathBuf {
        let agent_id_str = agent_id.to_string();
        let session_id_str = agent_id_str.split(':').next().unwrap_or("");

        let session_id =
            SessionUlid::from_string(session_id_str).unwrap_or_else(|_| SessionUlid::new());

        self.agents_dir(&session_id)
            .join(format!("{}.toml", agent_id))
    }

    /// Ensures a directory exists.
    async fn ensure_dir(&self, path: &Path) -> Result<(), StorageError> {
        if !path.exists() {
            fs::create_dir_all(path).await.map_err(StorageError::Io)?;
        }
        Ok(())
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn save_session(
        &self,
        session_meta: &SessionMeta,
        hierarchy_meta: &HierarchyMeta,
    ) -> Result<(), StorageError> {
        let session_dir = self.session_dir(&session_meta.id);
        self.ensure_dir(&session_dir).await?;

        let session_path = self.session_file(&session_meta.id);
        let session_content = toml::to_string_pretty(session_meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&session_path)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(session_content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        let hierarchy_path = self.hierarchy_file(&session_meta.id);
        let hierarchy_content = toml::to_string_pretty(hierarchy_meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&hierarchy_path)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(hierarchy_content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_session(&self, id: &SessionUlid) -> Result<Session, StorageError> {
        let session_path = self.session_file(id);
        let hierarchy_path = self.hierarchy_file(id);

        if !session_path.exists() {
            return Err(StorageError::NotFound(id.to_string()));
        }

        let mut session_file = fs::File::open(&session_path)
            .await
            .map_err(StorageError::Io)?;

        let mut session_content = String::new();
        session_file
            .read_to_string(&mut session_content)
            .await
            .map_err(StorageError::Io)?;

        let session_meta: SessionMeta = toml::from_str(&session_content)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut hierarchy_file = fs::File::open(&hierarchy_path)
            .await
            .map_err(StorageError::Io)?;

        let mut hierarchy_content = String::new();
        hierarchy_file
            .read_to_string(&mut hierarchy_content)
            .await
            .map_err(StorageError::Io)?;

        let hierarchy_meta: HierarchyMeta = toml::from_str(&hierarchy_content)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let hierarchy = crate::agent::AgentHierarchy::deserialize(hierarchy_meta);

        let session = Session::restore(crate::session::SessionRestoreParams {
            id: session_meta.id,
            session_type: session_meta.session_type,
            root_agent_ulid: session_meta.root_agent_ulid,
            hierarchy,
            id_allocator: crate::session::MessageIdAllocator::new(session_meta.next_message_id),
            metadata: session_meta.metadata,
            created_at: session_meta.created_at,
            updated_at: session_meta.updated_at,
        });

        Ok(session)
    }

    async fn delete_session(&self, id: &SessionUlid) -> Result<(), StorageError> {
        let session_dir = self.session_dir(id);
        if session_dir.exists() {
            fs::remove_dir_all(&session_dir)
                .await
                .map_err(StorageError::Io)?;
        }
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionUlid>, StorageError> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }

        let mut dir = fs::read_dir(&self.base_path)
            .await
            .map_err(StorageError::Io)?;

        let mut sessions = Vec::new();

        while let Some(entry) = dir.next_entry().await.map_err(StorageError::Io)? {
            let path = entry.path();
            if path.is_dir() {
                let dirname = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if let Ok(ulid) = SessionUlid::from_string(dirname) {
                    sessions.push(ulid);
                }
            }
        }

        Ok(sessions)
    }

    async fn save_agent(&self, agent: &Agent) -> Result<(), StorageError> {
        let agent_id_str = agent.id.to_string();
        let session_id_str = agent_id_str.split(':').next().unwrap_or("");

        let session_id =
            SessionUlid::from_string(session_id_str).unwrap_or_else(|_| SessionUlid::new());

        self.ensure_dir(&self.agents_dir(&session_id)).await?;

        let agent_file = self.agent_file(&agent.id);
        let content = toml::to_string_pretty(agent)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_agent(&self, id: &AgentUlid) -> Result<Agent, StorageError> {
        let agent_file = self.agent_file(id);

        if !agent_file.exists() {
            return Err(StorageError::NotFound(id.to_string()));
        }

        let mut file = fs::File::open(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(StorageError::Io)?;

        let agent: Agent =
            toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))?;

        Ok(agent)
    }

    async fn list_agents(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<Vec<AgentUlid>, StorageError> {
        let agents_dir = self.agents_dir(session_ulid);

        if !agents_dir.exists() {
            return Ok(Vec::new());
        }

        let mut dir = fs::read_dir(&agents_dir).await.map_err(StorageError::Io)?;

        let mut agents = Vec::new();

        while let Some(entry) = dir.next_entry().await.map_err(StorageError::Io)? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

                if let Ok(ulid) = AgentUlid::from_string(filename) {
                    agents.push(ulid);
                }
            }
        }

        Ok(agents)
    }

    async fn append_message(
        &self,
        agent_ulid: &AgentUlid,
        message: &Message,
    ) -> Result<(), StorageError> {
        let agent_file = self.agent_file(agent_ulid);

        let mut agent = if agent_file.exists() {
            let mut file = fs::File::open(&agent_file)
                .await
                .map_err(StorageError::Io)?;

            let mut content = String::new();
            file.read_to_string(&mut content)
                .await
                .map_err(StorageError::Io)?;

            toml::from_str::<Agent>(&content)
                .map_err(|e| StorageError::Serialization(e.to_string()))?
        } else {
            return Err(StorageError::NotFound(agent_ulid.to_string()));
        };

        agent.add_message(message.clone());

        let content = toml::to_string_pretty(&agent)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, StorageError> {
        let agent_file = self.agent_file(agent_ulid);

        if !agent_file.exists() {
            return Ok(Vec::new());
        }

        let mut file = fs::File::open(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(StorageError::Io)?;

        let agent: Agent =
            toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))?;

        Ok(agent.messages)
    }

    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), StorageError> {
        let agent_file = self.agent_file(agent_ulid);

        if !agent_file.exists() {
            return Err(StorageError::NotFound(agent_ulid.to_string()));
        }

        let mut file = fs::File::open(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(StorageError::Io)?;

        let mut agent: Agent =
            toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))?;

        let original_len = agent.messages.len();
        agent.messages.retain(|msg| msg.id >= before_id);

        if agent.messages.len() == original_len {
            return Ok(());
        }

        let content = toml::to_string_pretty(&agent)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }

    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), StorageError> {
        let agent_file = self.agent_file(agent_ulid);

        if !agent_file.exists() {
            return Err(StorageError::NotFound(agent_ulid.to_string()));
        }

        let mut file = fs::File::open(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        let mut content = String::new();
        file.read_to_string(&mut content)
            .await
            .map_err(StorageError::Io)?;

        let mut agent: Agent =
            toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))?;

        let original_len = agent.messages.len();
        agent.messages.retain(|msg| msg.id <= after_id);

        if agent.messages.len() == original_len {
            return Ok(());
        }

        let content = toml::to_string_pretty(&agent)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;

        let mut file = fs::File::create(&agent_file)
            .await
            .map_err(StorageError::Io)?;

        file.write_all(content.as_bytes())
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }
}
