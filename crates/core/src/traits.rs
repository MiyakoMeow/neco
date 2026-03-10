//! Domain traits and types for repository interfaces.

use crate::errors::{AgentError, RouteError, SessionError, StorageError};
use crate::events::{AgentState, SessionType};
use crate::ids::{AgentUlid, MessageId, SessionUlid};
use crate::messages::Message;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Session repository trait.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Create a new session.
    async fn create(&self, session: &Session) -> Result<(), SessionError>;
    /// Find session by ID.
    async fn find_by_id(&self, id: &SessionUlid) -> Result<Option<Session>, SessionError>;
    /// Get or create a session.
    async fn get_or_create(&self, session_ulid: &SessionUlid) -> Result<Session, SessionError>;
    /// Update an existing session.
    async fn update(&self, session: &Session) -> Result<(), SessionError>;
    /// Delete a session.
    async fn delete(&self, id: &SessionUlid) -> Result<(), SessionError>;
    /// List all sessions.
    async fn list(&self) -> Result<Vec<SessionUlid>, SessionError>;
}

/// Agent repository trait.
#[async_trait]
pub trait AgentRepository: Send + Sync {
    /// Create a new agent.
    async fn create(&self, agent: &Agent) -> Result<(), AgentError>;
    /// Find agent by ID.
    async fn find_by_id(&self, id: &AgentUlid) -> Result<Option<Agent>, AgentError>;
    /// Update an existing agent.
    async fn update(&self, agent: &Agent) -> Result<(), AgentError>;
    /// Delete an agent.
    async fn delete(&self, id: &AgentUlid) -> Result<(), AgentError>;
    /// Find agents by session.
    async fn find_by_session(&self, session_ulid: &SessionUlid) -> Result<Vec<Agent>, AgentError>;
    /// Find child agents.
    async fn find_children(&self, parent_ulid: &AgentUlid) -> Result<Vec<Agent>, AgentError>;
}

/// Message repository trait.
#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// Append a message to agent.
    async fn append(&self, agent_ulid: &AgentUlid, message: &Message) -> Result<(), StorageError>;
    /// List messages for agent.
    async fn list(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, StorageError>;
    /// Delete messages before given ID.
    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), StorageError>;
    /// Delete messages after given ID.
    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), StorageError>;
}

/// Session domain model.
pub struct Session {
    /// Session ULID.
    pub ulid: SessionUlid,
    /// Root agent ULID.
    pub root_agent_ulid: AgentUlid,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Session type.
    pub session_type: SessionType,
}

impl Session {
    /// Create a new session.
    pub fn new(ulid: SessionUlid, root_agent_ulid: AgentUlid, session_type: SessionType) -> Self {
        let now = Utc::now();
        Self {
            ulid,
            root_agent_ulid,
            created_at: now,
            updated_at: now,
            session_type,
        }
    }
}

/// Agent run mode (parsed representation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentModeParsed {
    /// Primary agent mode.
    Primary,
    /// Sub-agent mode.
    SubAgent,
    /// Multiple agents mode.
    Multiple,
}

/// Agent domain model.
#[derive(Debug, Clone)]
pub struct Agent {
    /// Agent ID.
    pub id: AgentUlid,
    /// Parent agent ID (None for root).
    pub parent_ulid: Option<AgentUlid>,
    /// Definition ID (reference to AgentDefinition).
    pub definition_id: Option<String>,
    /// Agent run mode.
    pub mode: AgentModeParsed,
    /// Model group reference.
    pub model_group: Option<String>,
    /// Custom system prompt.
    pub system_prompt: Option<String>,
    /// Current agent state.
    pub state: AgentState,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_activity: DateTime<Utc>,
}

impl Agent {
    /// Create a new agent.
    pub fn new(
        id: AgentUlid,
        parent_ulid: Option<AgentUlid>,
        definition_id: Option<String>,
        mode: AgentModeParsed,
        model_group: Option<String>,
        system_prompt: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            parent_ulid,
            definition_id,
            mode,
            model_group,
            system_prompt,
            state: crate::events::AgentState::Idle,
            created_at: now,
            last_activity: now,
        }
    }
}

/// Agent output type.
pub struct AgentOutput {
    /// Output content.
    pub content: String,
    /// Whether the agent is waiting for user input.
    pub waiting: bool,
}

/// Message routing service trait.
#[async_trait]
pub trait MessageRoutingService: Send + Sync {
    /// Route a message to the appropriate handler.
    async fn route_message(
        &self,
        session: &Session,
        message: &str,
    ) -> Result<AgentOutput, RouteError>;
}
