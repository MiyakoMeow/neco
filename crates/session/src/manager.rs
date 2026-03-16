//! Session manager module.

#![allow(clippy::pedantic)]

use crate::agent::{Agent, AgentModeParsed};
use crate::message::Message;
use crate::session::{Session, SessionError, SessionMeta, SessionMetadata, SessionType};
use crate::storage::{StorageBackend, StorageError};
use async_trait::async_trait;
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use std::sync::Arc;
use thiserror::Error;

/// Error type for session manager operations.
#[derive(Debug, Error)]
pub enum ManagerError {
    /// Session not found.
    #[error("Session不存在: {0}")]
    SessionNotFound(SessionUlid),

    /// Agent not found.
    #[error("Agent不存在: {0}")]
    AgentNotFound(AgentUlid),

    /// Storage error.
    #[error("存储错误: {0}")]
    Storage(#[from] StorageError),

    /// Session error.
    #[error("Session错误: {0}")]
    Session(#[from] SessionError),
}

impl ManagerError {
    /// Checks if the error is retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Storage(e) if e.is_retryable())
    }
}

/// Repository trait for session persistence.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Saves a session.
    async fn save(&self, session: &Session) -> Result<(), ManagerError>;
    /// Finds a session by ID.
    async fn find_by_id(&self, id: &SessionUlid) -> Result<Option<Session>, ManagerError>;
    /// Deletes a session.
    async fn delete(&self, id: &SessionUlid) -> Result<(), ManagerError>;
    /// Lists all sessions.
    async fn list(&self) -> Result<Vec<SessionUlid>, ManagerError>;
}

/// Repository trait for agent persistence.
#[async_trait]
pub trait AgentRepository: Send + Sync {
    /// Saves an agent.
    async fn save(&self, agent: &Agent) -> Result<(), ManagerError>;
    /// Finds an agent by ID.
    async fn find_by_id(&self, id: &AgentUlid) -> Result<Option<Agent>, ManagerError>;
    /// Finds agents by session.
    async fn find_by_session(&self, session_ulid: &SessionUlid)
    -> Result<Vec<Agent>, ManagerError>;
    /// Deletes an agent.
    async fn delete(&self, id: &AgentUlid) -> Result<(), ManagerError>;
}

/// Repository trait for message persistence.
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// Appends a message to an agent.
    async fn append(&self, agent_ulid: &AgentUlid, message: &Message) -> Result<(), ManagerError>;
    /// Lists messages for an agent.
    async fn list(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, ManagerError>;
    /// Deletes messages before a given ID.
    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), ManagerError>;
    /// Deletes messages after a given ID.
    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), ManagerError>;
}

/// Session manager for handling sessions and agents.
pub struct SessionManager<S: StorageBackend> {
    /// Storage backend.
    storage: Arc<S>,
}

impl<S: StorageBackend> SessionManager<S> {
    /// Creates a new SessionManager.
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    /// Creates a new session.
    pub async fn create_session(
        &self,
        session_type: SessionType,
        metadata: SessionMetadata,
    ) -> Result<Session, ManagerError> {
        let session = Session::new(session_type, metadata);
        let root_agent_id = *session.root_agent_ulid();

        let root_agent = Agent::new(
            root_agent_id,
            None,
            Some("root".to_string()),
            AgentModeParsed::Primary,
            None,
            None,
        );

        let session_meta = SessionMeta::from(&session);
        let hierarchy_meta = session.hierarchy().serialize();

        self.storage
            .save_session(&session_meta, &hierarchy_meta)
            .await?;
        self.storage.save_agent(&root_agent).await?;

        Ok(session)
    }

    /// Loads a session.
    pub async fn load_session(&self, session_ulid: &SessionUlid) -> Result<Session, ManagerError> {
        self.storage
            .load_session(session_ulid)
            .await
            .map_err(ManagerError::from)
    }

    /// Gets or creates a session.
    pub async fn get_or_create(&self, session_ulid: &SessionUlid) -> Result<Session, ManagerError> {
        match self.storage.load_session(session_ulid).await {
            Ok(session) => Ok(session),
            Err(StorageError::NotFound(_)) => {
                let session = Session::new(SessionType::default(), SessionMetadata::default());
                let session_meta = SessionMeta::from(&session);
                let hierarchy_meta = session.hierarchy().serialize();
                self.storage
                    .save_session(&session_meta, &hierarchy_meta)
                    .await
                    .map_err(ManagerError::from)?;
                Ok(session)
            },
            Err(e) => Err(ManagerError::Storage(e)),
        }
    }

    /// Deletes a session.
    pub async fn delete_session(&self, session_ulid: &SessionUlid) -> Result<(), ManagerError> {
        self.storage
            .delete_session(session_ulid)
            .await
            .map_err(ManagerError::from)
    }

    /// Loads an agent.
    pub async fn load_agent(&self, agent_ulid: &AgentUlid) -> Result<Agent, ManagerError> {
        self.storage
            .load_agent(agent_ulid)
            .await
            .map_err(|e| e.into())
    }

    /// Lists agents in a session.
    pub async fn list_agents(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<Vec<AgentUlid>, ManagerError> {
        self.storage
            .list_agents(session_ulid)
            .await
            .map_err(|e| e.into())
    }

    /// Adds a message to an agent.
    pub async fn add_message(
        &self,
        agent_ulid: &AgentUlid,
        message: Message,
    ) -> Result<(), ManagerError> {
        self.storage
            .append_message(agent_ulid, &message)
            .await
            .map_err(|e| e.into())
    }

    /// Gets messages for an agent.
    pub async fn get_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, ManagerError> {
        self.storage
            .load_messages(agent_ulid)
            .await
            .map_err(|e| e.into())
    }

    /// Spawns a new agent.
    pub async fn spawn_agent(
        &self,
        session: &mut Session,
        parent_ulid: AgentUlid,
        definition_id: String,
    ) -> Result<Agent, ManagerError> {
        let agent_ulid = session.spawn_agent(parent_ulid)?;
        let agent = Agent::new(
            agent_ulid,
            Some(parent_ulid),
            Some(definition_id),
            AgentModeParsed::SubAgent,
            None,
            None,
        );

        self.storage
            .save_agent(&agent)
            .await
            .map_err(ManagerError::from)?;

        let session_meta = SessionMeta::from(&*session);
        let hierarchy_meta = session.hierarchy().serialize();
        self.storage
            .save_session(&session_meta, &hierarchy_meta)
            .await
            .map_err(ManagerError::from)?;

        Ok(agent)
    }
}

#[async_trait]
impl<S: StorageBackend> neoco_core::kernel::SessionManager for SessionManager<S> {
    async fn create_session(
        &self,
        _config: neoco_core::kernel::SessionConfig,
    ) -> Result<
        (neoco_core::ids::SessionUlid, neoco_core::ids::AgentUlid),
        neoco_core::kernel::KernelError,
    > {
        let session_type = crate::SessionType::Direct {
            initial_message: None,
        };
        let metadata = crate::SessionMetadata::default();
        let session = self
            .create_session(session_type, metadata)
            .await
            .map_err(|e| neoco_core::kernel::KernelError::Config(e.to_string()))?;
        Ok((*session.id(), *session.root_agent_ulid()))
    }

    async fn load_session(
        &self,
        session_id: neoco_core::ids::SessionUlid,
    ) -> Result<neoco_core::traits::Session, neoco_core::kernel::KernelError> {
        let session = self
            .load_session(&session_id)
            .await
            .map_err(|e| neoco_core::kernel::KernelError::Config(e.to_string()))?;
        Ok(neoco_core::traits::Session {
            ulid: *session.id(),
            root_agent_ulid: *session.root_agent_ulid(),
            created_at: session.created_at(),
            updated_at: session.updated_at(),
            session_type: neoco_core::events::SessionType::Direct {
                initial_message: None,
            },
        })
    }

    async fn delete_session(
        &self,
        session_id: neoco_core::ids::SessionUlid,
    ) -> Result<(), neoco_core::kernel::KernelError> {
        self.delete_session(&session_id)
            .await
            .map_err(|e| neoco_core::kernel::KernelError::Config(e.to_string()))
    }

    async fn get_root_agent(
        &self,
        session_id: neoco_core::ids::SessionUlid,
    ) -> Result<neoco_core::ids::AgentUlid, neoco_core::kernel::KernelError> {
        let session = self
            .load_session(&session_id)
            .await
            .map_err(|e| neoco_core::kernel::KernelError::Config(e.to_string()))?;
        Ok(*session.root_agent_ulid())
    }
}
