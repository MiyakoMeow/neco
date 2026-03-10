//! In-memory storage implementation.

#![allow(clippy::pedantic)]

use crate::agent::{Agent, AgentHierarchy, AgentModeParsed, HierarchyMeta};
use crate::message::Message;
use crate::session::{MessageIdAllocator, Session, SessionMeta, SessionRestoreParams};
use crate::storage::{StorageBackend, StorageError};
use async_trait::async_trait;
use neoco_core::ids::{AgentUlid, MessageId, SessionUlid};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory storage.
pub struct InMemoryStorage {
    /// Sessions storage.
    sessions: Arc<RwLock<HashMap<SessionUlid, (SessionMeta, HierarchyMeta)>>>,
    /// Agents storage.
    agents: Arc<RwLock<HashMap<AgentUlid, Agent>>>,
    /// Messages storage.
    messages: Arc<RwLock<HashMap<AgentUlid, Vec<Message>>>>,
}

impl InMemoryStorage {
    /// Creates a new InMemoryStorage.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            agents: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl StorageBackend for InMemoryStorage {
    async fn save_session(
        &self,
        session_meta: &SessionMeta,
        hierarchy_meta: &HierarchyMeta,
    ) -> Result<(), StorageError> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            session_meta.id,
            (session_meta.clone(), hierarchy_meta.clone()),
        );
        Ok(())
    }

    async fn load_session(&self, id: &SessionUlid) -> Result<Session, StorageError> {
        let sessions = self.sessions.read().await;
        let (session_meta, hierarchy_meta) = sessions
            .get(id)
            .ok_or_else(|| StorageError::NotFound(id.to_string()))?;

        let hierarchy = AgentHierarchy::deserialize(hierarchy_meta.clone());

        let messages = self.messages.read().await;
        let root_messages = messages
            .get(&session_meta.root_agent_ulid)
            .cloned()
            .unwrap_or_default();

        let mut agent = Agent::new(
            session_meta.root_agent_ulid,
            None,
            Some("root".to_string()),
            AgentModeParsed::Primary,
            None,
            None,
        );
        for msg in root_messages {
            agent.add_message(msg);
        }

        let agents = self.agents.read().await;
        let mut all_agents = vec![agent];
        for (ulid, agent) in agents.iter() {
            if ulid != &session_meta.root_agent_ulid {
                all_agents.push(agent.clone());
            }
        }

        let session = Session::restore(SessionRestoreParams {
            id: session_meta.id,
            session_type: session_meta.session_type.clone(),
            root_agent_ulid: session_meta.root_agent_ulid,
            hierarchy,
            id_allocator: MessageIdAllocator::new(session_meta.next_message_id),
            metadata: session_meta.metadata.clone(),
            created_at: session_meta.created_at,
            updated_at: session_meta.updated_at,
        });

        Ok(session)
    }

    async fn delete_session(&self, id: &SessionUlid) -> Result<(), StorageError> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionUlid>, StorageError> {
        let sessions = self.sessions.read().await;
        Ok(sessions.keys().copied().collect())
    }

    async fn save_agent(&self, agent: &Agent) -> Result<(), StorageError> {
        let mut agents = self.agents.write().await;
        agents.insert(agent.id, agent.clone());
        Ok(())
    }

    async fn load_agent(&self, id: &AgentUlid) -> Result<Agent, StorageError> {
        let agents = self.agents.read().await;
        agents
            .get(id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    async fn list_agents(
        &self,
        session_ulid: &SessionUlid,
    ) -> Result<Vec<AgentUlid>, StorageError> {
        let sessions = self.sessions.read().await;
        let (_session_meta, _) = sessions
            .get(session_ulid)
            .ok_or_else(|| StorageError::NotFound(session_ulid.to_string()))?;

        let agents = self.agents.read().await;
        let result: Vec<AgentUlid> = agents
            .keys()
            .filter(|ulid| {
                let ulid_str = ulid.to_string();
                ulid_str.starts_with(&session_ulid.to_string())
            })
            .copied()
            .collect();

        Ok(result)
    }

    async fn append_message(
        &self,
        agent_ulid: &AgentUlid,
        message: &Message,
    ) -> Result<(), StorageError> {
        let mut messages = self.messages.write().await;
        messages
            .entry(*agent_ulid)
            .or_insert_with(Vec::new)
            .push(message.clone());
        Ok(())
    }

    async fn load_messages(&self, agent_ulid: &AgentUlid) -> Result<Vec<Message>, StorageError> {
        let messages = self.messages.read().await;
        Ok(messages.get(agent_ulid).cloned().unwrap_or_default())
    }

    async fn delete_prefix(
        &self,
        agent_ulid: &AgentUlid,
        before_id: MessageId,
    ) -> Result<(), StorageError> {
        let mut messages = self.messages.write().await;
        if let Some(msgs) = messages.get_mut(agent_ulid) {
            msgs.retain(|msg| msg.id >= before_id);
        }
        Ok(())
    }

    async fn delete_suffix(
        &self,
        agent_ulid: &AgentUlid,
        after_id: MessageId,
    ) -> Result<(), StorageError> {
        let mut messages = self.messages.write().await;
        if let Some(msgs) = messages.get_mut(agent_ulid) {
            msgs.retain(|msg| msg.id <= after_id);
        }
        Ok(())
    }
}
