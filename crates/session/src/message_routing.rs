//! Message routing service module.
//!
//! This module provides message routing functionality for the `NeoCo` session system.

use async_trait::async_trait;
use neoco_core::errors::RouteError;
use neoco_core::ids::AgentUlid;
use neoco_core::traits::AgentOutput;
use neoco_core::traits::MessageRoutingService as CoreMessageRoutingService;
use neoco_core::traits::Session;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Message type for routing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    /// Task message requiring agent action.
    Task,
    /// Progress update message.
    Progress,
    /// Result message.
    Result,
    /// Clarification request message.
    Clarification,
    /// Unknown message type.
    Unknown,
}

impl MessageType {
    /// Determines message type from content.
    #[must_use]
    pub fn from_content(content: &str) -> Self {
        let lower = content.to_lowercase();
        if lower.contains("@task") || lower.contains("[task]") || lower.starts_with("task:") {
            MessageType::Task
        } else if lower.contains("@progress") || lower.contains("[progress]") {
            MessageType::Progress
        } else if lower.contains("@result") || lower.contains("[result]") {
            MessageType::Result
        } else if lower.contains("@clarification") || lower.contains("[?]") {
            MessageType::Clarification
        } else {
            MessageType::Unknown
        }
    }
}

/// Routing target containing agent(s) to route to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingTarget {
    /// List of agent IDs to route to.
    pub agent_ids: Vec<AgentUlid>,
    /// Whether this is a broadcast to all targets.
    pub broadcast: bool,
}

impl RoutingTarget {
    /// Creates a single-agent routing target.
    #[must_use]
    pub fn single(agent_id: AgentUlid) -> Self {
        Self {
            agent_ids: vec![agent_id],
            broadcast: false,
        }
    }

    /// Creates a broadcast routing target.
    #[must_use]
    pub fn broadcast(agent_ids: Vec<AgentUlid>) -> Self {
        Self {
            agent_ids,
            broadcast: true,
        }
    }
}

/// Configuration for message routing.
#[derive(Debug, Clone)]
pub struct MessageRoutingConfig {
    /// Whether to default to root agent when no other target found.
    pub default_to_root: bool,
    /// Whether to allow broadcast routing.
    pub allow_broadcast: bool,
    /// Whether to route by @mention in message.
    pub route_by_mention: bool,
}

impl Default for MessageRoutingConfig {
    fn default() -> Self {
        Self {
            default_to_root: true,
            allow_broadcast: true,
            route_by_mention: true,
        }
    }
}

/// Agent hierarchy information for routing.
#[derive(Debug, Clone)]
pub struct AgentHierarchyInfo {
    /// Map of agent ID to parent ID.
    pub parent_map: HashMap<String, String>,
    /// Map of parent ID to list of child IDs.
    pub children_map: HashMap<String, Vec<String>>,
}

/// Message routing service for routing messages to appropriate agents.
pub struct MessageRoutingService {
    /// Routing configuration.
    config: MessageRoutingConfig,
    /// Agent hierarchies by session ID.
    agent_hierarchies: HashMap<String, AgentHierarchyInfo>,
}

impl MessageRoutingService {
    /// Creates a new `MessageRoutingService` with default config.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: MessageRoutingConfig::default(),
            agent_hierarchies: HashMap::new(),
        }
    }

    /// Creates a new `MessageRoutingService` with custom config.
    #[must_use]
    pub fn with_config(config: MessageRoutingConfig) -> Self {
        Self {
            config,
            agent_hierarchies: HashMap::new(),
        }
    }

    /// Registers agent hierarchy for a session.
    pub fn register_hierarchy(
        &mut self,
        session_ulid: &str,
        root_agent_ulid: AgentUlid,
        children: &[AgentUlid],
    ) {
        let mut children_map = HashMap::new();
        children_map.insert(
            root_agent_ulid.to_string(),
            children.iter().map(AgentUlid::to_string).collect(),
        );

        let mut parent_map = HashMap::new();
        for child in children {
            parent_map.insert(child.to_string(), root_agent_ulid.to_string());
        }

        self.agent_hierarchies.insert(
            session_ulid.to_string(),
            AgentHierarchyInfo {
                parent_map,
                children_map,
            },
        );
    }

    /// Checks if an agent exists in the hierarchy.
    fn has_agent(
        &self,
        session_ulid: &str,
        root_agent_ulid: AgentUlid,
        agent_id: &AgentUlid,
    ) -> bool {
        if let Some(hierarchy) = self.agent_hierarchies.get(session_ulid) {
            if hierarchy.children_map.contains_key(&agent_id.to_string()) {
                return true;
            }
            if hierarchy.parent_map.contains_key(&agent_id.to_string()) {
                return true;
            }
            return false;
        }
        *agent_id == root_agent_ulid
    }

    /// Gets children of an agent.
    fn get_children(&self, session_ulid: &str, agent_id: &AgentUlid) -> Vec<AgentUlid> {
        if let Some(hierarchy) = self.agent_hierarchies.get(session_ulid)
            && let Some(children) = hierarchy.children_map.get(&agent_id.to_string())
        {
            return children.iter().filter_map(|s| s.parse().ok()).collect();
        }
        Vec::new()
    }

    /// Extracts agent mentions from message content.
    fn extract_agent_mentions(content: &str) -> Vec<AgentUlid> {
        let mut mentions = Vec::new();
        let agent_patterns = ["@agent:", "@subagent:", "@child:"];

        for pattern in agent_patterns {
            if let Some(rest) = content.to_lowercase().strip_prefix(pattern) {
                let end = rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(rest.len());
                let id_str = &rest[..end];
                if let Ok(ulid) = id_str.parse::<AgentUlid>() {
                    mentions.push(ulid);
                }
            }
        }

        mentions
    }

    /// Determines routing target based on message content and session hierarchy.
    fn determine_routing_target(
        &self,
        session_ulid: &str,
        root_agent_ulid: AgentUlid,
        message: &str,
    ) -> Result<RoutingTarget, RouteError> {
        let message_type = MessageType::from_content(message);

        if self.config.route_by_mention {
            let mentions = Self::extract_agent_mentions(message);
            if !mentions.is_empty() {
                let valid_mentions: Vec<AgentUlid> = mentions
                    .into_iter()
                    .filter(|id| self.has_agent(session_ulid, root_agent_ulid, id))
                    .collect();

                if !valid_mentions.is_empty() {
                    return Ok(RoutingTarget::broadcast(valid_mentions));
                }
            }
        }

        match message_type {
            MessageType::Task => {
                let children = self.get_children(session_ulid, &root_agent_ulid);
                if !children.is_empty() && self.config.allow_broadcast {
                    return Ok(RoutingTarget::broadcast(children));
                }
            },
            MessageType::Progress | MessageType::Result => {
                let children = self.get_children(session_ulid, &root_agent_ulid);
                if let Some(&first) = children.first() {
                    return Ok(RoutingTarget::single(first));
                }
            },
            MessageType::Clarification => {
                return Ok(RoutingTarget::single(root_agent_ulid));
            },
            MessageType::Unknown => {},
        }

        if self.config.default_to_root {
            Ok(RoutingTarget::single(root_agent_ulid))
        } else {
            Err(RouteError::NotFound(
                "No valid routing target found".to_string(),
            ))
        }
    }

    /// Routes a message to a specific agent.
    ///
    /// # Errors
    ///
    /// Returns `RouteError::NotFound` if the agent is not found in the session.
    pub fn route_to_agent(
        &self,
        session_ulid: &str,
        root_agent_ulid: AgentUlid,
        agent_id: &AgentUlid,
        message: &str,
    ) -> Result<AgentOutput, RouteError> {
        if !self.has_agent(session_ulid, root_agent_ulid, agent_id) {
            return Err(RouteError::NotFound(format!(
                "Agent {agent_id} not found in session {session_ulid}",
            )));
        }

        Ok(AgentOutput {
            content: format!("Message routed to agent {agent_id}: {message}"),
            waiting: false,
        })
    }

    /// Routes a message to all children of a specific agent.
    ///
    /// # Errors
    ///
    /// Returns `RouteError::NotFound` if no child agents are found.
    pub fn route_to_all_children(
        &self,
        session_ulid: &str,
        _root_agent_ulid: AgentUlid,
        parent_id: &AgentUlid,
        message: &str,
    ) -> Result<Vec<AgentOutput>, RouteError> {
        let children = self.get_children(session_ulid, parent_id);

        if children.is_empty() {
            return Err(RouteError::NotFound(format!(
                "No child agents found for parent {parent_id}",
            )));
        }

        let mut results = Vec::new();
        for child_id in children {
            results.push(AgentOutput {
                content: format!("Broadcast to child agent {child_id}: {message}"),
                waiting: false,
            });
        }

        Ok(results)
    }

    /// Synchronously routes a message to appropriate agent(s).
    ///
    /// # Errors
    ///
    /// Returns an error if routing fails.
    pub fn route_message_sync(
        &self,
        session: &Session,
        message: &str,
    ) -> Result<AgentOutput, RouteError> {
        let session_ulid = session.ulid.to_string();
        let root_agent_ulid = session.root_agent_ulid;

        let target = self.determine_routing_target(&session_ulid, root_agent_ulid, message)?;

        if target.broadcast {
            let mut count = 0;
            for agent_id in &target.agent_ids {
                let children = self.get_children(&session_ulid, agent_id);
                if children.is_empty() {
                    count += 1;
                } else {
                    count += children.len();
                }
            }

            if count == 0 {
                return Err(RouteError::Failed(
                    "Broadcast resulted in no targets".to_string(),
                ));
            }

            Ok(AgentOutput {
                content: format!("Broadcast to {count} agents: {message}"),
                waiting: false,
            })
        } else {
            let agent_id = target
                .agent_ids
                .first()
                .ok_or_else(|| RouteError::NotFound("No target agent specified".to_string()))?;

            Ok(AgentOutput {
                content: format!("Route to {agent_id}: {message}"),
                waiting: false,
            })
        }
    }
}

impl Default for MessageRoutingService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CoreMessageRoutingService for MessageRoutingService {
    async fn route_message(
        &self,
        session: &Session,
        message: &str,
    ) -> Result<AgentOutput, RouteError> {
        self.route_message_sync(session, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_core::events::SessionType;
    use neoco_core::ids::SessionUlid;

    fn create_test_session() -> Session {
        let session_id = SessionUlid::new();
        let root_id = AgentUlid::new_root(&session_id);
        Session::new(session_id, root_id, SessionType::Tui)
    }

    #[test]
    fn test_message_type_from_content_task() {
        assert_eq!(
            MessageType::from_content("@task: do something"),
            MessageType::Task
        );
        assert_eq!(
            MessageType::from_content("[task] do something"),
            MessageType::Task
        );
        assert_eq!(
            MessageType::from_content("TASK: do something"),
            MessageType::Task
        );
    }

    #[test]
    fn test_message_type_from_content_progress() {
        assert_eq!(
            MessageType::from_content("@progress: 50%"),
            MessageType::Progress
        );
        assert_eq!(
            MessageType::from_content("[progress] updating"),
            MessageType::Progress
        );
    }

    #[test]
    fn test_message_type_from_content_result() {
        assert_eq!(
            MessageType::from_content("@result: done"),
            MessageType::Result
        );
        assert_eq!(
            MessageType::from_content("[result] completed"),
            MessageType::Result
        );
    }

    #[test]
    fn test_message_type_from_content_clarification() {
        assert_eq!(
            MessageType::from_content("@clarification: help"),
            MessageType::Clarification
        );
        assert_eq!(
            MessageType::from_content("[?] question"),
            MessageType::Clarification
        );
    }

    #[test]
    fn test_message_type_from_content_unknown() {
        assert_eq!(
            MessageType::from_content("hello world"),
            MessageType::Unknown
        );
        assert_eq!(MessageType::from_content(""), MessageType::Unknown);
    }

    #[test]
    fn test_routing_target_single() {
        let session = SessionUlid::new();
        let agent_id = AgentUlid::new_root(&session);
        let target = RoutingTarget::single(agent_id);

        assert_eq!(target.agent_ids.len(), 1);
        assert!(!target.broadcast);
    }

    #[test]
    fn test_routing_target_broadcast() {
        let session = SessionUlid::new();
        let agent1 = AgentUlid::new_root(&session);
        let agent2 = AgentUlid::new_child(&agent1);
        let target = RoutingTarget::broadcast(vec![agent1, agent2]);

        assert_eq!(target.agent_ids.len(), 2);
        assert!(target.broadcast);
    }

    #[tokio::test]
    async fn test_route_message_default_to_root() {
        let session = create_test_session();
        let routing = MessageRoutingService::new();

        let result = routing.route_message(&session, "hello").await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output
                .content
                .contains(&session.root_agent_ulid.to_string())
        );
    }

    #[tokio::test]
    async fn test_route_message_with_hierarchy() {
        let session_id = SessionUlid::new();
        let root_id = AgentUlid::new_root(&session_id);
        let child_id = AgentUlid::new_child(&root_id);

        let session = Session::new(session_id, root_id, SessionType::Tui);

        let mut routing = MessageRoutingService::new();
        routing.register_hierarchy(&session_id.to_string(), root_id, &[child_id]);

        let result = routing.route_message(&session, "@task: do something").await;

        result.unwrap();
    }

    #[test]
    fn test_route_to_agent_not_found() {
        let session = create_test_session();
        let routing = MessageRoutingService::new();

        let fake_id = AgentUlid::new_root(&SessionUlid::new());
        let result = routing.route_to_agent(
            &session.ulid.to_string(),
            session.root_agent_ulid,
            &fake_id,
            "test",
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_message_routing_config_default() {
        let config = MessageRoutingConfig::default();
        assert!(config.default_to_root);
        assert!(config.allow_broadcast);
        assert!(config.route_by_mention);
    }

    #[test]
    fn test_message_routing_config_custom() {
        let config = MessageRoutingConfig {
            default_to_root: false,
            allow_broadcast: false,
            route_by_mention: false,
        };
        assert!(!config.default_to_root);
        assert!(!config.allow_broadcast);
        assert!(!config.route_by_mention);
    }

    #[tokio::test]
    async fn test_route_message_clarification() {
        let session_id = SessionUlid::new();
        let root_id = AgentUlid::new_root(&session_id);

        let session = Session::new(session_id, root_id, SessionType::Tui);

        let routing = MessageRoutingService::new();
        let result = routing
            .route_message(&session, "[?] need clarification")
            .await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.content.contains(&root_id.to_string()));
    }

    #[test]
    fn test_route_to_all_children() {
        let session_id = SessionUlid::new();
        let root_id = AgentUlid::new_root(&session_id);
        let child_id = AgentUlid::new_child(&root_id);

        let mut routing = MessageRoutingService::new();
        routing.register_hierarchy(&session_id.to_string(), root_id, &[child_id]);

        let result = routing.route_to_all_children(
            &session_id.to_string(),
            root_id,
            &root_id,
            "broadcast msg",
        );

        assert!(result.is_ok());
        let outputs = result.unwrap();
        assert_eq!(outputs.len(), 1);
    }
}
