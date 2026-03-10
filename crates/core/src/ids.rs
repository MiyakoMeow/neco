//! Strong type identifiers using newtype pattern.

use crate::errors::IdError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smol_str::SmolStr;
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;
use ulid::Ulid;

/// Session identifier using ULID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionUlid(Ulid);

impl SessionUlid {
    /// Create a new SessionUlid with a randomly generated ULID.
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Create a SessionUlid from an existing Ulid.
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Parse a SessionUlid from a string.
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        let ulid = Ulid::from_str(s).map_err(|e| IdError::ParseError {
            input: s.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self(ulid))
    }

    /// Get a reference to the underlying Ulid.
    pub fn as_ulid(&self) -> &Ulid {
        &self.0
    }

    /// Convert to string representation.
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SessionUlid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionUlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionUlid {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SessionUlid::from_string(s)
    }
}

impl Serialize for SessionUlid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SessionUlid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SessionUlid::from_string(&s).map_err(serde::de::Error::custom)
    }
}

/// Agent identifier with session and agent ULIDs.
///
/// The session field directly identifies the parent Session,
/// and the agent field uniquely identifies the Agent instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentUlid {
    /// Session ULID.
    session: Ulid,
    /// Agent ULID.
    agent: Ulid,
}

impl AgentUlid {
    /// Create a new root AgentUlid for a session.
    pub fn new_root(session_ulid: &SessionUlid) -> Self {
        Self {
            session: *session_ulid.as_ulid(),
            agent: Ulid::new(),
        }
    }

    /// Create a new child AgentUlid under a parent.
    pub fn new_child(parent: &AgentUlid) -> Self {
        Self {
            session: parent.session,
            agent: Ulid::new(),
        }
    }

    /// Create from session and agent ULIDs.
    pub fn from_ulids(session: Ulid, agent: Ulid) -> Self {
        Self { session, agent }
    }

    /// Parse from string format "session:agent".
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(IdError::InvalidFormat(s.to_string()));
        }
        let session = Ulid::from_str(parts[0]).map_err(|e| IdError::ParseError {
            input: s.to_string(),
            reason: e.to_string(),
        })?;
        let agent = Ulid::from_str(parts[1]).map_err(|e| IdError::ParseError {
            input: s.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self { session, agent })
    }

    /// Get session ULID reference.
    pub fn session(&self) -> &Ulid {
        &self.session
    }

    /// Get agent ULID reference.
    pub fn agent(&self) -> &Ulid {
        &self.agent
    }

    /// Convert to SessionUlid.
    pub fn as_session_ulid(&self) -> SessionUlid {
        SessionUlid(self.session)
    }

    /// Convert to string format.
    pub fn to_string(&self) -> String {
        format!("{}:{}", self.session, self.agent)
    }
}

impl fmt::Display for AgentUlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl FromStr for AgentUlid {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AgentUlid::from_string(s)
    }
}

impl Serialize for AgentUlid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for AgentUlid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        AgentUlid::from_string(&s).map_err(serde::de::Error::custom)
    }
}

/// Unique message identifier using atomic auto-increment.
///
/// Globally unique across all agents and workflow nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(NonZeroU64);

impl MessageId {
    /// Create a new MessageId starting from 1.
    pub fn new() -> Self {
        Self(NonZeroU64::new(1).unwrap())
    }

    /// Create from u64 value.
    pub fn from_u64(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    /// Get the underlying u64 value.
    pub fn as_u64(&self) -> u64 {
        self.0.get()
    }

    /// Increment and return new MessageId.
    pub fn increment(&self) -> Self {
        Self(NonZeroU64::new(self.0.get() + 1).unwrap())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MessageId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<u64>().map_err(|_| IdError::ParseError {
            input: s.to_string(),
            reason: "invalid u64".to_string(),
        })?;
        MessageId::from_u64(value).ok_or(IdError::ValidationFailed(
            "MessageId cannot be zero".to_string(),
        ))
    }
}

impl Serialize for MessageId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for MessageId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        MessageId::from_u64(value).ok_or(serde::de::Error::custom("MessageId cannot be zero"))
    }
}

/// Node identifier using ULID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeUlid(Ulid);

impl NodeUlid {
    /// Create a new NodeUlid with a randomly generated ULID.
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Create from an existing Ulid.
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Parse from string.
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        let ulid = Ulid::from_str(s).map_err(|e| IdError::ParseError {
            input: s.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self(ulid))
    }

    /// Get reference to underlying Ulid.
    pub fn as_ulid(&self) -> &Ulid {
        &self.0
    }

    /// Convert to string.
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for NodeUlid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeUlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for NodeUlid {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NodeUlid::from_string(s)
    }
}

impl Serialize for NodeUlid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for NodeUlid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NodeUlid::from_string(&s).map_err(serde::de::Error::custom)
    }
}

/// Tool identifier in namespace::name format.
/// Uses SmolStr for efficient short string storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolId(SmolStr);

impl ToolId {
    /// Create from namespace parts.
    #[deprecated(since = "0.2.0", note = "Use from_string instead")]
    pub fn new(namespaces: &[String]) -> Self {
        Self(SmolStr::from(namespaces.join("::")))
    }

    /// Create from string slices.
    #[deprecated(since = "0.2.0", note = "Use from_string instead")]
    pub fn from_parts(namespaces: &[&str]) -> Self {
        Self(SmolStr::from(namespaces.join("::")))
    }

    /// Create from namespace and name with validation.
    pub fn from_parts_validated(namespace: &str, name: &str) -> Result<Self, IdError> {
        if namespace.is_empty() {
            return Err(IdError::ValidationFailed(
                "namespace cannot be empty".to_string(),
            ));
        }
        if name.is_empty() {
            return Err(IdError::ValidationFailed(
                "name cannot be empty".to_string(),
            ));
        }
        if !namespace
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(IdError::ValidationFailed(format!(
                "Invalid namespace '{}': only lowercase letters, digits, and hyphens allowed",
                namespace
            )));
        }
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(IdError::ValidationFailed(format!(
                "Invalid name '{}': only lowercase letters, digits, hyphens, and underscores allowed",
                name
            )));
        }
        let s = format!("{namespace}::{name}");
        Ok(Self(SmolStr::from(s)))
    }

    /// Parse from string (e.g., "fs::read").
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        if s.is_empty() {
            return Err(IdError::Empty);
        }
        Ok(Self(SmolStr::from(s)))
    }

    /// Get namespace parts.
    #[deprecated(since = "0.2.0", note = "Use to_string for simple access")]
    pub fn namespaces(&self) -> Vec<String> {
        self.0.split("::").map(String::from).collect()
    }

    /// Get the tool namespace (first part).
    pub fn namespace(&self) -> Option<&str> {
        self.0.split("::").next()
    }

    /// Get the tool name (last part).
    pub fn name(&self) -> Option<&str> {
        self.0.split("::").last()
    }

    /// Convert to string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ToolId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ToolId::from_string(s)
    }
}

impl Serialize for ToolId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ToolId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ToolId::from_string(&s).map_err(serde::de::Error::custom)
    }
}

/// Skill identifier using ULID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SkillUlid(Ulid);

impl SkillUlid {
    /// Create a new SkillUlid with randomly generated ULID.
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Create from existing Ulid.
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Parse from string.
    pub fn from_string(s: &str) -> Result<Self, IdError> {
        let ulid = Ulid::from_str(s).map_err(|e| IdError::ParseError {
            input: s.to_string(),
            reason: e.to_string(),
        })?;
        Ok(Self(ulid))
    }

    /// Get reference to underlying Ulid.
    pub fn as_ulid(&self) -> &Ulid {
        &self.0
    }

    /// Convert to string.
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SkillUlid {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SkillUlid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SkillUlid {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SkillUlid::from_string(s)
    }
}

impl Serialize for SkillUlid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SkillUlid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        SkillUlid::from_string(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_ulid_new() {
        let id = SessionUlid::new();
        assert!(!id.to_string().is_empty());
    }

    #[test]
    fn test_session_ulid_from_string() {
        let id = SessionUlid::new();
        let parsed = SessionUlid::from_string(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_session_ulid_invalid_string() {
        let result = SessionUlid::from_string("invalid");
        result.unwrap_err();
    }

    #[test]
    fn test_agent_ulid_root() {
        let session = SessionUlid::new();
        let agent = AgentUlid::new_root(&session);
        assert_eq!(*agent.session(), *session.as_ulid());
    }

    #[test]
    fn test_agent_ulid_child() {
        let session = SessionUlid::new();
        let parent = AgentUlid::new_root(&session);
        let child = AgentUlid::new_child(&parent);
        assert_eq!(*child.session(), *parent.session());
    }

    #[test]
    fn test_agent_ulid_to_string() {
        let session = SessionUlid::new();
        let agent = AgentUlid::new_root(&session);
        let s = agent.to_string();
        assert!(s.contains(':'));
    }

    #[test]
    fn test_message_id_increment() {
        let id = MessageId::new();
        let next = id.increment();
        assert_eq!(next.as_u64(), id.as_u64() + 1);
    }

    #[test]
    fn test_message_id_from_u64() {
        let id = MessageId::from_u64(42).unwrap();
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_message_id_zero() {
        let id = MessageId::from_u64(0);
        assert!(id.is_none());
    }

    #[test]
    fn test_tool_id_from_string() {
        let tool = ToolId::from_string("fs::read").unwrap();
        assert_eq!(tool.namespace(), Some("fs"));
        assert_eq!(tool.name(), Some("read"));
    }

    #[test]
    fn test_tool_id_to_string() {
        let tool = ToolId::from_string("mcp::github").unwrap();
        assert_eq!(tool.to_string(), "mcp::github");
    }

    #[test]
    fn test_tool_id_empty() {
        let result = ToolId::from_string("");
        result.unwrap_err();
    }

    #[test]
    fn test_tool_id_from_parts_validated_success() {
        let tool = ToolId::from_parts_validated("fs", "read").unwrap();
        assert_eq!(tool.namespace(), Some("fs"));
        assert_eq!(tool.name(), Some("read"));
    }

    #[test]
    fn test_tool_id_from_parts_validated_invalid_namespace() {
        let result = ToolId::from_parts_validated("Fs", "read");
        result.unwrap_err();
    }

    #[test]
    fn test_tool_id_from_parts_validated_invalid_name() {
        let result = ToolId::from_parts_validated("fs", "Read");
        result.unwrap_err();
    }

    #[test]
    fn test_tool_id_from_parts_validated_empty_namespace() {
        let result = ToolId::from_parts_validated("", "read");
        result.unwrap_err();
    }

    #[test]
    fn test_tool_id_from_parts_validated_empty_name() {
        let result = ToolId::from_parts_validated("fs", "");
        result.unwrap_err();
    }

    #[test]
    fn test_skill_ulid_roundtrip() {
        let id = SkillUlid::new();
        let parsed = SkillUlid::from_string(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_node_ulid_roundtrip() {
        let id = NodeUlid::new();
        let parsed = NodeUlid::from_string(&id.to_string()).unwrap();
        assert_eq!(id, parsed);
    }
}
