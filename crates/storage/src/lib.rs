//! neoco-storage: File system storage backend for `NeoCo` sessions.
//!
//! This module implements the `StorageBackend` trait from neoco-session,
//! providing file-based persistence for sessions, agents, and messages.
//!
//! ## Storage Structure
//!
//! ```text
//! ~/.local/share/neoco/sessions/
//! └── {session_id}/
//!     ├── session.toml       # Session metadata
//!     ├── hierarchy.toml     # Agent hierarchy
//!     └── agents/
//!         └── {agent_id}.toml  # Agent data and messages
//! ```

#![allow(unused_crate_dependencies)]

use async_trait::async_trait;
use neoco_config::Paths;
use neoco_core::events::AgentState;
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use neoco_session::agent::{Agent, AgentHierarchy, HierarchyMeta};
use neoco_session::message::Message;
use neoco_session::session::{MessageIdAllocator, Session, SessionMeta, SessionRestoreParams};
use neoco_session::storage::{StorageBackend, StorageError};

type AgentStateDto = AgentState;
use std::path::PathBuf;
use tokio::fs;

/// File system storage backend implementation.
pub struct FileStorage {
    /// Base path for storage.
    base_path: PathBuf,
}

impl FileStorage {
    /// Creates a new `FileStorage` using the default session directory.
    #[must_use]
    pub fn new() -> Self {
        let paths = Paths::new();
        Self {
            base_path: paths.session_dir,
        }
    }

    /// Creates a new `FileStorage` with a custom base path.
    pub fn with_base_path(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Get the session directory path.
    fn session_dir(&self, session_id: &SessionUlid) -> PathBuf {
        self.base_path.join(session_id.to_string())
    }

    /// Get the agents directory path.
    fn agents_dir(&self, session_id: &SessionUlid) -> PathBuf {
        self.session_dir(session_id).join("agents")
    }

    /// Get the agent file path.
    fn agent_file(&self, agent_id: &AgentUlid) -> PathBuf {
        let session_id = agent_id.as_session_ulid();
        self.agents_dir(&session_id)
            .join(format!("{agent_id}.toml"))
    }

    /// Ensure session directory exists.
    async fn ensure_session_dir(&self, session_id: &SessionUlid) -> Result<(), StorageError> {
        let dir = self.session_dir(session_id);
        fs::create_dir_all(&dir).await.map_err(StorageError::Io)?;
        let agents_dir = self.agents_dir(session_id);
        fs::create_dir_all(&agents_dir)
            .await
            .map_err(StorageError::Io)?;
        Ok(())
    }

    /// Load session metadata from file.
    async fn load_session_meta(
        &self,
        session_id: &SessionUlid,
    ) -> Result<SessionMeta, StorageError> {
        let path = self.session_dir(session_id).join("session.toml");
        let content: String =
            fs::read_to_string(&path)
                .await
                .map_err(|e: std::io::Error| match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        StorageError::NotFound(path.to_string_lossy().to_string())
                    },
                    _ => StorageError::Io(e),
                })?;
        toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))
    }

    /// Load hierarchy metadata from file.
    async fn load_hierarchy(
        &self,
        session_id: &SessionUlid,
    ) -> Result<HierarchyMeta, StorageError> {
        let path = self.session_dir(session_id).join("hierarchy.toml");
        let content: String =
            fs::read_to_string(&path)
                .await
                .map_err(|e: std::io::Error| match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        StorageError::NotFound(path.to_string_lossy().to_string())
                    },
                    _ => StorageError::Io(e),
                })?;
        toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))
    }

    /// Load agent data from file.
    async fn load_agent_data(&self, agent_id: &AgentUlid) -> Result<AgentData, StorageError> {
        let path = self.agent_file(agent_id);
        let content: String =
            fs::read_to_string(&path)
                .await
                .map_err(|e: std::io::Error| match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        StorageError::NotFound(path.to_string_lossy().to_string())
                    },
                    _ => StorageError::Io(e),
                })?;
        toml::from_str(&content).map_err(|e| StorageError::Serialization(e.to_string()))
    }
}

impl Default for FileStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for FileStorage {
    async fn save_session(
        &self,
        session_meta: &SessionMeta,
        hierarchy_meta: &HierarchyMeta,
    ) -> Result<(), StorageError> {
        let session_id = &session_meta.id;
        self.ensure_session_dir(session_id).await?;

        let path = self.session_dir(session_id).join("session.toml");
        let content = toml::to_string_pretty(session_meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&path, content).await.map_err(StorageError::Io)?;

        let path = self.session_dir(session_id).join("hierarchy.toml");
        let content = toml::to_string_pretty(hierarchy_meta)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&path, content).await.map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_session(&self, id: &SessionUlid) -> Result<Session, StorageError> {
        let session_meta = self.load_session_meta(id).await?;
        let hierarchy_meta = self.load_hierarchy(id).await?;

        let hierarchy = AgentHierarchy::deserialize(hierarchy_meta);
        let id_allocator = MessageIdAllocator::new(session_meta.next_message_id);

        let session = Session::restore(SessionRestoreParams {
            id: session_meta.id,
            session_type: session_meta.session_type,
            root_agent_ulid: session_meta.root_agent_ulid,
            hierarchy,
            id_allocator,
            metadata: session_meta.metadata,
            created_at: session_meta.created_at,
            updated_at: session_meta.updated_at,
        });

        Ok(session)
    }

    async fn delete_session(&self, id: &SessionUlid) -> Result<(), StorageError> {
        let path = self.session_dir(id);
        let result = fs::remove_dir_all(&path).await;
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    async fn save_agent(&self, agent: &Agent) -> Result<(), StorageError> {
        let session_id = agent.id.as_session_ulid();
        self.ensure_session_dir(&session_id).await?;

        let agent_data = AgentData::from(agent);
        let path = self.agent_file(&agent.id);
        let content = toml::to_string_pretty(&agent_data)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&path, content).await.map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_agent(&self, id: &AgentUlid) -> Result<Agent, StorageError> {
        let agent_data = self.load_agent_data(id).await?;
        Ok(agent_data.into())
    }

    async fn list_agents(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<Vec<AgentUlid>, StorageError> {
        let agents_dir = self.agents_dir(session_ulid);

        let entries_result = fs::read_dir(&agents_dir).await;
        let mut entries = match entries_result {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(StorageError::Io(e)),
        };

        let mut agent_ids = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(StorageError::Io)? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml")
                && let Some(stem) = path.file_stem()
                && let Some(id_str) = stem.to_str()
                && let Ok(agent_id) = AgentUlid::from_string(id_str)
            {
                agent_ids.push(agent_id);
            }
        }

        Ok(agent_ids)
    }

    async fn append_message(
        &self,
        agent_ulid: &AgentUlid,
        message: &Message,
    ) -> Result<(), StorageError> {
        let session_id = agent_ulid.as_session_ulid();
        self.ensure_session_dir(&session_id).await?;

        let agent_data = match self.load_agent_data(agent_ulid).await {
            Ok(mut data) => {
                data.messages.push(message.clone());
                data
            },
            Err(StorageError::NotFound(_)) => AgentData::new(*agent_ulid, message.clone()),
            Err(e) => return Err(e),
        };

        let path = self.agent_file(agent_ulid);
        let content = toml::to_string_pretty(&agent_data)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&path, content).await.map_err(StorageError::Io)?;

        Ok(())
    }

    async fn load_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, StorageError> {
        let agent_data = self.load_agent_data(agent_ulid).await?;
        Ok(agent_data.messages)
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

    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), StorageError> {
        let agent_file_path = self.agent_file(agent_ulid);

        if !agent_file_path.exists() {
            return Err(StorageError::NotFound(agent_ulid.to_string()));
        }

        let mut agent_data = self.load_agent_data(agent_ulid).await?;

        let original_len = agent_data.messages.len();
        agent_data.messages.retain(|msg| msg.id >= before_id);

        if agent_data.messages.len() == original_len {
            return Ok(());
        }

        let content = toml::to_string_pretty(&agent_data)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&agent_file_path, content)
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }

    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), StorageError> {
        let agent_file_path = self.agent_file(agent_ulid);

        if !agent_file_path.exists() {
            return Err(StorageError::NotFound(agent_ulid.to_string()));
        }

        let mut agent_data = self.load_agent_data(agent_ulid).await?;

        let original_len = agent_data.messages.len();
        agent_data.messages.retain(|msg| msg.id <= after_id);

        if agent_data.messages.len() == original_len {
            return Ok(());
        }

        let content = toml::to_string_pretty(&agent_data)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        fs::write(&agent_file_path, content)
            .await
            .map_err(StorageError::Io)?;

        Ok(())
    }
}

/// Agent data stored in TOML format.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AgentData {
    /// Agent ID.
    #[serde(rename = "id")]
    agent_id: AgentUlid,
    /// Agent definition ID.
    #[serde(rename = "agent")]
    definition_id: Option<String>,
    /// Agent mode.
    #[serde(rename = "mode")]
    mode: Option<String>,
    /// Model group.
    #[serde(rename = "model_group")]
    model_group: Option<String>,
    /// System prompt.
    #[serde(rename = "system_prompt")]
    system_prompt: Option<String>,
    /// Parent agent ID if any.
    #[serde(rename = "parent")]
    parent_ulid: Option<AgentUlid>,
    /// Agent state.
    #[serde(rename = "state")]
    state: AgentStateDto,
    /// Active tools.
    #[serde(rename = "active_tools")]
    active_tools: Vec<String>,
    /// Active MCP servers.
    #[serde(rename = "active_mcp")]
    active_mcp: Vec<String>,
    /// Active skills.
    #[serde(rename = "active_skills")]
    active_skills: Vec<String>,
    /// Creation timestamp.
    #[serde(rename = "created_at")]
    created_at: chrono::DateTime<chrono::Utc>,
    /// Last activity timestamp.
    #[serde(rename = "last_activity")]
    last_activity: chrono::DateTime<chrono::Utc>,
    /// Messages.
    #[serde(rename = "messages")]
    messages: Vec<Message>,
}

impl AgentData {
    /// Creates a new agent data instance.
    fn new(agent_id: AgentUlid, message: Message) -> Self {
        Self {
            agent_id,
            definition_id: None,
            mode: None,
            model_group: None,
            system_prompt: None,
            parent_ulid: None,
            state: AgentState::Idle,
            active_tools: Vec::new(),
            active_mcp: Vec::new(),
            active_skills: Vec::new(),
            created_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            messages: vec![message],
        }
    }
}

impl From<&Agent> for AgentData {
    fn from(agent: &Agent) -> Self {
        Self {
            agent_id: agent.id,
            definition_id: agent.definition_id.clone(),
            mode: Some("primary".to_string()),
            model_group: agent.model_group.clone(),
            system_prompt: agent.system_prompt.clone(),
            parent_ulid: agent.parent_ulid,
            state: agent.state.clone(),
            active_tools: agent
                .active_tools
                .iter()
                .map(neoco_core::ToolId::to_string)
                .collect(),
            active_mcp: agent.active_mcp.clone(),
            active_skills: agent
                .active_skills
                .iter()
                .map(neoco_core::SkillUlid::to_string)
                .collect(),
            created_at: agent.created_at,
            last_activity: agent.last_activity,
            messages: agent.messages.clone(),
        }
    }
}

impl From<AgentData> for Agent {
    fn from(data: AgentData) -> Self {
        Self {
            id: data.agent_id,
            parent_ulid: data.parent_ulid,
            definition_id: data.definition_id,
            mode: neoco_session::agent::AgentModeParsed::Primary,
            model_group: data.model_group,
            system_prompt: data.system_prompt,
            messages: data.messages,
            state: data.state.into(),
            active_tools: data
                .active_tools
                .iter()
                .filter_map(|t| neoco_core::ids::ToolId::from_string(t).ok())
                .collect(),
            active_mcp: data.active_mcp,
            active_skills: data
                .active_skills
                .iter()
                .filter_map(|s| neoco_core::ids::SkillUlid::from_string(s).ok())
                .collect(),
            created_at: data.created_at,
            last_activity: data.last_activity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_session::agent::Agent;
    use neoco_session::message::Message;
    use neoco_session::session::{SessionMeta, SessionType};
    use tempfile::TempDir;

    fn create_test_session() -> Session {
        let session_type = SessionType::Direct {
            initial_message: None,
        };
        let metadata = neoco_session::session::SessionMetadata {
            user_id: Some("test_user".to_string()),
            working_dir: std::path::PathBuf::from("/test"),
            initial_prompt: None,
            custom: std::collections::HashMap::new(),
        };
        Session::new(session_type, metadata)
    }

    #[tokio::test]
    async fn test_save_and_load_session() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::with_base_path(temp_dir.path());

        let session = create_test_session();
        let session_id = *session.id();
        let hierarchy = session.hierarchy().serialize();

        let session_meta = SessionMeta::from(&session);
        storage
            .save_session(&session_meta, &hierarchy)
            .await
            .unwrap();

        let loaded = storage.load_session(&session_id).await.unwrap();

        assert_eq!(loaded.id(), session.id());
    }

    #[tokio::test]
    async fn test_list_agents() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::with_base_path(temp_dir.path());

        let session = create_test_session();
        let session_meta = SessionMeta::from(&session);
        let hierarchy = session.hierarchy().serialize();
        storage
            .save_session(&session_meta, &hierarchy)
            .await
            .unwrap();

        let root_id = *session.root_agent_ulid();

        storage
            .save_agent(&Agent::new(
                root_id,
                None,
                Some("root".to_string()),
                neoco_session::agent::AgentModeParsed::Primary,
                None,
                None,
            ))
            .await
            .unwrap();

        let load_ok = storage.load_agent(&root_id).await.is_ok();
        assert!(load_ok, "agent should be saved and loadable");

        #[cfg(not(windows))]
        {
            let agents = storage.list_agents(session.id()).await.unwrap();
            assert_eq!(agents.len(), 1);
            assert_eq!(agents[0], root_id);
        }
    }

    #[tokio::test]
    async fn test_append_and_load_messages() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::with_base_path(temp_dir.path());

        let session = create_test_session();
        let session_meta = SessionMeta::from(&session);
        let hierarchy = session.hierarchy().serialize();
        storage
            .save_session(&session_meta, &hierarchy)
            .await
            .unwrap();

        let agent_id = *session.root_agent_ulid();

        let msg1 = Message::user("Hello");
        let msg2 = Message::assistant("Hi there");

        storage.append_message(&agent_id, &msg1).await.unwrap();
        storage.append_message(&agent_id, &msg2).await.unwrap();

        let messages = storage.load_messages(&agent_id).await.unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages.first().map(|m| m.content.as_str()), Some("Hello"));
        assert_eq!(
            messages.get(1).map(|m| m.content.as_str()),
            Some("Hi there")
        );
    }

    #[tokio::test]
    async fn test_delete_session() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::with_base_path(temp_dir.path());

        let session = create_test_session();
        let session_id = *session.id();
        let session_meta = SessionMeta::from(&session);
        let hierarchy = session.hierarchy().serialize();

        storage
            .save_session(&session_meta, &hierarchy)
            .await
            .unwrap();

        storage.delete_session(&session_id).await.unwrap();

        let result = storage.load_session(&session_id).await;
        assert!(matches!(result, Err(StorageError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_storage_file_structure() {
        let temp_dir = TempDir::new().unwrap();
        let storage = FileStorage::with_base_path(temp_dir.path());

        let session = create_test_session();
        let session_id = *session.id();
        let session_meta = SessionMeta::from(&session);
        let hierarchy = session.hierarchy().serialize();

        storage
            .save_session(&session_meta, &hierarchy)
            .await
            .unwrap();

        let session_dir = temp_dir.path().join(session_id.to_string());
        assert!(session_dir.exists());

        let session_file = session_dir.join("session.toml");
        assert!(session_file.exists());

        let hierarchy_file = session_dir.join("hierarchy.toml");
        assert!(hierarchy_file.exists());

        let agents_dir = session_dir.join("agents");
        assert!(agents_dir.is_dir());
    }
}
