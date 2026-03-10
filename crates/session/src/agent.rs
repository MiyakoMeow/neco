//! Agent domain model module.

use chrono::{DateTime, Utc};
use neoco_core::events::AgentState;
use neoco_core::ids::{AgentUlid, SkillUlid, ToolId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

/// MCP server identifier.
pub type McpServerId = String;

/// Agent state transition error.
#[derive(Debug, Error)]
pub enum StateTransitionError {
    /// Invalid state transition.
    #[error("无效的状态转换: {from:?} -> {to:?}")]
    InvalidTransition {
        /// Source state.
        from: AgentState,
        /// Target state.
        to: AgentState,
    },
}

/// Agent definition parsing error.
#[derive(Debug, Error)]
pub enum AgentDefinitionError {
    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// YAML parsing error.
    #[error("YAML parsing error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    /// TOML parsing error.
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),
    /// Invalid format error.
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
}

/// Prompts configuration for TOML \[prompts\] section support.
///
/// Supports both formats:
/// - TOML: \[prompts\] base = true, multi-agent = true
/// - YAML: prompts: [base, multi-agent]
///
/// The config is stored as a map of prompt name to enabled status.
/// When accessed via  method, returns only enabled prompts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsConfig(HashMap<String, bool>);

impl PromptsConfig {
    /// Create a new `PromptsConfig` from a list of prompt names.
    #[must_use]
    pub fn from_list(names: &[String]) -> Self {
        Self(names.iter().map(|n| (n.clone(), true)).collect())
    }

    /// Convert to a list of enabled prompt names.
    #[must_use]
    pub fn to_list(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get enabled prompts as a `Vec<String>` for compatibility.
    #[must_use]
    pub fn prompts(&self) -> Vec<String> {
        self.to_list()
    }

    /// Check if a specific prompt is enabled.
    #[must_use]
    pub fn is_enabled(&self, name: &str) -> bool {
        self.0.get(name).copied().unwrap_or(false)
    }

    /// Enable a prompt.
    pub fn enable(&mut self, name: impl Into<String>) {
        self.0.insert(name.into(), true);
    }

    /// Disable a prompt.
    pub fn disable(&mut self, name: impl Into<String>) {
        self.0.insert(name.into(), false);
    }

    /// Check if the config is empty (no prompts enabled).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() || !self.0.values().any(|&v| v)
    }
}

impl From<Vec<String>> for PromptsConfig {
    fn from(list: Vec<String>) -> Self {
        Self::from_list(&list)
    }
}

impl From<PromptsConfig> for Vec<String> {
    fn from(config: PromptsConfig) -> Vec<String> {
        config.to_list()
    }
}

/// Agent definition from configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentDefinition {
    /// Agent ID.
    pub id: Option<String>,
    /// Agent description.
    pub description: Option<String>,
    /// Agent mode.
    pub mode: AgentMode,
    /// Model value (model name or reference).
    pub model: ModelValue,
    /// Temperature setting.
    pub temperature: Option<f64>,
    /// Timeout duration.
    pub timeout: Option<Duration>,
    /// Model group/provider.
    pub model_group: Option<String>,
    /// System prompt (custom system prompt override).
    pub system_prompt: Option<String>,
    /// Prompt templates (legacy format, for backward compatibility).
    /// Use `prompts_config` for TOML \[prompts\] section support.
    pub prompts: Vec<String>,
    /// Prompts configuration for TOML \[prompts\] section support.
    /// When deserialized from TOML, this stores the prompt name -> enabled status map.
    /// When deserialized from YAML, the `prompts` field is used instead.
    #[serde(default, skip_serializing_if = "PromptsConfig::is_empty")]
    pub prompts_config: PromptsConfig,
    /// MCP server names.
    pub mcp_servers: Vec<String>,
    /// Skill names.
    pub skills: Vec<String>,
    /// Extra fields.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// Model value representation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelValue {
    /// Model name as string.
    String(String),
    /// Model reference object.
    Object(ModelRef),
    /// No model specified.
    #[default]
    None,
}

/// Model reference with provider and name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Model provider.
    pub provider: String,
    /// Model name.
    pub name: String,
    /// Temperature setting.
    pub temperature: Option<f64>,
}

/// Agent mode representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentMode {
    /// Single mode as string.
    String(String),
    /// Multiple modes as array.
    Array(Vec<String>),
    /// Parsed mode.
    Parsed(AgentModeParsed),
}

impl Default for AgentMode {
    fn default() -> Self {
        Self::String("primary".to_string())
    }
}

/// Parsed agent mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentModeParsed {
    /// Primary agent.
    Primary,
    /// Sub-agent.
    SubAgent,
    /// Multiple agents.
    Multiple(Vec<AgentModeParsed>),
}

impl AgentDefinition {
    /// Resolves the model group from the definition.
    #[must_use]
    pub fn resolve_model_group(&self) -> Option<&str> {
        if let Some(ref mg) = self.model_group {
            return Some(mg);
        }

        match &self.model {
            ModelValue::String(s) => s.split('/').next(),
            ModelValue::Object(o) => Some(&o.provider),
            ModelValue::None => None,
        }
    }

    /// Resolves the model name from the definition.
    #[must_use]
    pub fn resolve_model_name(&self) -> Option<&str> {
        match &self.model {
            ModelValue::String(s) => s.split('/').nth(1),
            ModelValue::Object(o) => Some(&o.name),
            ModelValue::None => None,
        }
    }

    /// Resolves the temperature setting.
    #[must_use]
    pub fn resolve_temperature(&self) -> Option<f64> {
        match &self.model {
            ModelValue::Object(o) => o.temperature.or(self.temperature),
            _ => self.temperature,
        }
    }

    /// Resolves the agent mode.
    #[must_use]
    pub fn resolve_mode(&self) -> AgentModeParsed {
        match &self.mode {
            AgentMode::String(s) => match s.as_str() {
                "subagent" => AgentModeParsed::SubAgent,
                _ => AgentModeParsed::Primary,
            },
            AgentMode::Array(arr) => {
                if arr.is_empty() {
                    AgentModeParsed::Primary
                } else {
                    let parsed: Vec<AgentModeParsed> = arr
                        .iter()
                        .map(|s| match s.as_str() {
                            "subagent" => AgentModeParsed::SubAgent,
                            _ => AgentModeParsed::Primary,
                        })
                        .collect();
                    AgentModeParsed::Multiple(parsed)
                }
            },
            AgentMode::Parsed(p) => p.clone(),
        }
    }

    /// Load Agent definition from a file.
    ///
    /// Supports YAML frontmatter + Markdown body format:
    /// ```yaml
    /// ---
    /// id: agent-id
    /// model: provider/name
    /// temperature: 0.7
    /// ...
    /// ---
    /// # Markdown body
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the format is invalid.
    pub fn from_file(path: &Path) -> Result<ParsedAgentFile, AgentDefinitionError> {
        let content = std::fs::read_to_string(path)?;

        let (frontmatter, body) = if let Some(stripped) = content.strip_prefix("---") {
            let end_marker_pos = stripped.find("\n---").ok_or_else(|| {
                AgentDefinitionError::InvalidFormat(
                    "Missing closing '---' in frontmatter".to_string(),
                )
            })?;

            let frontmatter = content[3..3 + end_marker_pos].trim().to_string();
            let body = content[3 + end_marker_pos + 4..].trim_start().to_string();
            (frontmatter, body)
        } else {
            (String::new(), content)
        };

        let mut definition: AgentDefinition = if frontmatter.is_empty() {
            AgentDefinition::default()
        } else {
            serde_yaml::from_str(&frontmatter)?
        };

        if definition.id.is_none()
            && let Some(stem) = path.file_stem()
        {
            definition.id = Some(stem.to_string_lossy().to_string());
        }

        if !body.is_empty() {
            definition.prompts.insert(0, body.clone());
        }

        Ok(ParsedAgentFile { definition, body })
    }

    /// Load Agent definition from a TOML file.
    ///
    /// Supports pure TOML format with \[prompts\] section:
    /// ```toml
    /// [prompts]
    /// base = true
    /// multi-agent = true
    ///
    /// [agent]
    /// id = "agent-id"
    /// model = "provider/name"
    /// temperature = 0.7
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or the format is invalid.
    pub fn from_toml_file(path: &Path) -> Result<AgentDefinition, AgentDefinitionError> {
        let content = std::fs::read_to_string(path)?;

        // Try to parse as TOML
        let mut definition: AgentDefinition = toml::from_str(&content)?;

        // If no ID set, use filename stem
        if definition.id.is_none()
            && let Some(stem) = path.file_stem()
        {
            definition.id = Some(stem.to_string_lossy().to_string());
        }

        // If prompts_config is set but prompts is empty, convert prompts_config to prompts list
        if !definition.prompts_config.is_empty() && definition.prompts.is_empty() {
            definition.prompts = definition.prompts_config.to_list();
        }

        Ok(definition)
    }

    /// Get all prompts (combining legacy prompts field and `prompts_config`).
    ///
    /// Returns a list of all enabled prompt names.
    #[must_use]
    pub fn get_prompts(&self) -> Vec<String> {
        // If prompts_config has entries, use it
        if !self.prompts_config.is_empty() {
            return self.prompts_config.to_list();
        }
        // Otherwise, use legacy prompts field
        self.prompts.clone()
    }

    /// Sets the agent mode.
    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
    }

    /// Sets the temperature setting.
    pub fn set_temperature(&mut self, temperature: Option<f64>) {
        self.temperature = temperature;
    }

    /// Sets the timeout duration.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        self.timeout = timeout;
    }
}

/// Agent file parsing result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedAgentFile {
    /// Agent metadata definition.
    pub definition: AgentDefinition,
    /// Markdown body content (used as prompt).
    pub body: String,
}

/// Agent domain model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Agent ID.
    pub id: AgentUlid,
    /// Parent agent ID.
    pub parent_ulid: Option<AgentUlid>,
    /// Definition ID (reference to `AgentDefinition`).
    pub definition_id: Option<String>,
    /// Agent run mode.
    pub mode: AgentModeParsed,
    /// Model group reference.
    pub model_group: Option<String>,
    /// Custom system prompt.
    pub system_prompt: Option<String>,
    /// Messages in this agent's context.
    pub messages: Vec<crate::message::Message>,
    /// Current state.
    pub state: AgentState,
    /// Active tool IDs.
    pub active_tools: Vec<ToolId>,
    /// Active MCP servers.
    pub active_mcp: Vec<McpServerId>,
    /// Active skill IDs.
    pub active_skills: Vec<SkillUlid>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp.
    pub last_activity: DateTime<Utc>,
}

impl Agent {
    /// Creates a new agent.
    #[must_use]
    pub fn new(
        id: AgentUlid,
        parent_ulid: Option<AgentUlid>,
        definition_id: Option<String>,
        mode: AgentModeParsed,
        model_group: Option<String>,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            id,
            parent_ulid,
            definition_id,
            mode,
            model_group,
            system_prompt,
            messages: Vec::new(),
            state: AgentState::Idle,
            active_tools: Vec::new(),
            active_mcp: Vec::new(),
            active_skills: Vec::new(),
            created_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }

    /// Adds a message to the agent.
    pub fn add_message(&mut self, message: crate::message::Message) {
        self.last_activity = Utc::now();
        self.messages.push(message);
    }

    /// Sets the agent state with validation.
    pub fn set_state(&mut self, new_state: AgentState) -> Result<(), StateTransitionError> {
        if !self.state.can_transition_to(&new_state) {
            return Err(StateTransitionError::InvalidTransition {
                from: self.state.clone(),
                to: new_state,
            });
        }
        self.state = new_state;
        self.last_activity = Utc::now();
        Ok(())
    }

    /// Sets the model group.
    pub fn set_model_group(&mut self, model_group: Option<String>) {
        self.model_group = model_group;
        self.last_activity = Utc::now();
    }

    /// Sets the system prompt.
    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
        self.last_activity = Utc::now();
    }

    /// Sets the parent agent ULID.
    pub fn set_parent_ulid(&mut self, parent_ulid: Option<AgentUlid>) {
        self.parent_ulid = parent_ulid;
        self.last_activity = Utc::now();
    }
}

/// Agent hierarchy for managing parent-child relationships between agents.
#[derive(Debug, Clone)]
pub struct AgentHierarchy {
    /// Root agent ULID.
    root: AgentUlid,
    /// Map of child ULID to parent ULID.
    parent_map: HashMap<AgentUlid, AgentUlid>,
    /// Map of parent ULID to list of child ULIDs.
    children_map: HashMap<AgentUlid, Vec<AgentUlid>>,
}

impl AgentHierarchy {
    /// Creates a new hierarchy with the given root agent.
    #[must_use]
    pub fn new(root: AgentUlid) -> Self {
        let mut children_map = HashMap::new();
        children_map.insert(root, Vec::new());
        Self {
            root,
            parent_map: HashMap::new(),
            children_map,
        }
    }

    /// Adds a child agent to the hierarchy.
    pub fn add_child(&mut self, parent: AgentUlid, child: AgentUlid) {
        self.parent_map.insert(child, parent);
        self.children_map.entry(parent).or_default().push(child);
    }

    /// Checks if an agent exists in the hierarchy.
    #[must_use]
    pub fn has_agent(&self, id: &AgentUlid) -> bool {
        *id == self.root || self.parent_map.contains_key(id)
    }

    /// Gets the parent of an agent.
    #[must_use]
    pub fn get_parent(&self, id: &AgentUlid) -> Option<AgentUlid> {
        self.parent_map.get(id).copied()
    }

    /// Gets the children of an agent.
    #[must_use]
    pub fn get_children(&self, id: &AgentUlid) -> Vec<AgentUlid> {
        self.children_map.get(id).cloned().unwrap_or_default()
    }

    /// Gets all ancestors of an agent.
    #[must_use]
    pub fn get_ancestors(&self, id: &AgentUlid) -> Vec<AgentUlid> {
        let mut ancestors = Vec::new();
        let mut current = *id;

        while let Some(parent) = self.get_parent(&current) {
            ancestors.push(parent);
            current = parent;
        }

        ancestors
    }

    /// Gets all descendants of an agent.
    #[must_use]
    pub fn get_descendants(&self, id: &AgentUlid) -> Vec<AgentUlid> {
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();

        for child in self.get_children(id) {
            queue.push_back(child);
        }

        while let Some(current) = queue.pop_front() {
            result.push(current);
            for child in self.get_children(&current) {
                queue.push_back(child);
            }
        }

        result
    }

    /// Returns the root agent.
    #[must_use]
    pub fn root(&self) -> AgentUlid {
        self.root
    }

    /// Serializes the hierarchy to metadata.
    #[must_use]
    pub fn serialize(&self) -> HierarchyMeta {
        HierarchyMeta {
            root: self.root,
            parent_map: self.parent_map.clone(),
            children_map: self.children_map.clone(),
        }
    }

    /// Deserializes a hierarchy from metadata.
    #[must_use]
    pub fn deserialize(meta: HierarchyMeta) -> Self {
        Self {
            root: meta.root,
            parent_map: meta.parent_map,
            children_map: meta.children_map,
        }
    }
}

/// Hierarchy metadata for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyMeta {
    /// Root agent ID.
    pub root: AgentUlid,
    /// Parent map.
    pub parent_map: HashMap<AgentUlid, AgentUlid>,
    /// Children map.
    pub children_map: HashMap<AgentUlid, Vec<AgentUlid>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use neoco_core::ids::SessionUlid;

    #[test]
    fn test_agent_creation() {
        let session = SessionUlid::new();
        let agent_id = AgentUlid::new_root(&session);

        let agent = Agent::new(
            agent_id,
            None,
            Some("test_agent".to_string()),
            AgentModeParsed::Primary,
            None,
            None,
        );

        assert_eq!(agent.definition_id, Some("test_agent".to_string()));
        assert_eq!(agent.state, AgentState::Idle);
    }

    #[test]
    fn test_agent_add_message() {
        let session = SessionUlid::new();
        let agent_id = AgentUlid::new_root(&session);

        let mut agent = Agent::new(
            agent_id,
            None,
            Some("test".to_string()),
            AgentModeParsed::Primary,
            None,
            None,
        );

        let msg = crate::message::Message::user("Hello");
        agent.add_message(msg);

        assert_eq!(agent.messages.len(), 1);
    }

    #[test]
    fn test_agent_hierarchy_new() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let hierarchy = AgentHierarchy::new(root);

        assert!(hierarchy.has_agent(&root));
    }

    #[test]
    fn test_agent_hierarchy_add_child() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let mut hierarchy = AgentHierarchy::new(root);
        let child = AgentUlid::new_child(&root);

        hierarchy.add_child(root, child);

        assert!(hierarchy.has_agent(&child));
        assert_eq!(hierarchy.get_parent(&child), Some(root));
    }

    #[test]
    fn test_agent_hierarchy_get_children() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let mut hierarchy = AgentHierarchy::new(root);
        let child1 = AgentUlid::new_child(&root);
        let child2 = AgentUlid::new_child(&root);

        hierarchy.add_child(root, child1);
        hierarchy.add_child(root, child2);

        let children = hierarchy.get_children(&root);
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_agent_hierarchy_get_ancestors() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let mut hierarchy = AgentHierarchy::new(root);
        let child1 = AgentUlid::new_child(&root);
        let child2 = AgentUlid::new_child(&child1);

        hierarchy.add_child(root, child1);
        hierarchy.add_child(child1, child2);

        let ancestors = hierarchy.get_ancestors(&child2);
        assert!(ancestors.contains(&child1));
        assert!(ancestors.contains(&root));
    }

    #[test]
    fn test_agent_hierarchy_get_descendants() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let mut hierarchy = AgentHierarchy::new(root);
        let child1 = AgentUlid::new_child(&root);
        let child2 = AgentUlid::new_child(&root);
        let child1_1 = AgentUlid::new_child(&child1);

        hierarchy.add_child(root, child1);
        hierarchy.add_child(root, child2);
        hierarchy.add_child(child1, child1_1);

        let descendants = hierarchy.get_descendants(&root);
        assert!(descendants.contains(&child1));
        assert!(descendants.contains(&child2));
        assert!(descendants.contains(&child1_1));
    }

    #[test]
    fn test_agent_hierarchy_serialize() {
        let session = SessionUlid::new();
        let root = AgentUlid::new_root(&session);

        let mut hierarchy = AgentHierarchy::new(root);
        let child = AgentUlid::new_child(&root);
        hierarchy.add_child(root, child);

        let meta = hierarchy.serialize();
        let restored = AgentHierarchy::deserialize(meta);

        assert!(restored.has_agent(&child));
    }

    #[test]
    fn test_agent_definition_resolve_model() {
        let def = AgentDefinition {
            model: ModelValue::String("openai/gpt-4".to_string()),
            model_group: Some("openai".to_string()),
            ..Default::default()
        };

        assert_eq!(def.resolve_model_group(), Some("openai"));
        assert_eq!(def.resolve_model_name(), Some("gpt-4"));
    }

    #[test]
    fn test_agent_definition_resolve_temperature() {
        let def = AgentDefinition {
            model: ModelValue::Object(ModelRef {
                provider: "openai".to_string(),
                name: "gpt-4".to_string(),
                temperature: Some(0.5),
            }),
            temperature: Some(0.7),
            ..Default::default()
        };

        assert_eq!(def.resolve_temperature(), Some(0.5));
    }

    #[test]
    fn test_agent_definition_resolve_mode() {
        let def1 = AgentDefinition {
            mode: AgentMode::String("subagent".to_string()),
            ..Default::default()
        };
        assert!(matches!(def1.resolve_mode(), AgentModeParsed::SubAgent));

        let def2 = AgentDefinition {
            mode: AgentMode::Array(vec!["primary".to_string(), "subagent".to_string()]),
            ..Default::default()
        };
        assert!(matches!(def2.resolve_mode(), AgentModeParsed::Multiple(_)));
    }

    #[test]
    fn test_agent_definition_from_file_with_frontmatter() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_agent.md");

        let content = r"---
id: test-agent
description: Test agent
model: openai/gpt-4
temperature: 0.7
mcp_servers:
  - mcp1
skills:
  - skill1
prompts:
  - prompt1
---
# Body content";

        std::fs::write(&test_file, content).unwrap();

        let result = AgentDefinition::from_file(&test_file).unwrap();

        assert_eq!(result.definition.id, Some("test-agent".to_string()));
        assert_eq!(
            result.definition.description,
            Some("Test agent".to_string())
        );
        assert!(
            matches!(result.definition.model, ModelValue::String(ref s) if s == "openai/gpt-4")
        );
        assert_eq!(result.definition.temperature, Some(0.7));
        assert_eq!(result.definition.mcp_servers, vec!["mcp1"]);
        assert_eq!(result.definition.skills, vec!["skill1"]);
        assert!(result.body.contains("Body content"));
        assert!(
            result
                .definition
                .prompts
                .contains(&"# Body content".to_string())
        );

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_agent_definition_from_file_without_frontmatter() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("simple_agent.md");

        let content = "# Simple Agent Prompt";

        std::fs::write(&test_file, content).unwrap();

        let result = AgentDefinition::from_file(&test_file).unwrap();

        assert_eq!(result.definition.id, Some("simple_agent".to_string()));
        assert!(
            result
                .definition
                .prompts
                .contains(&"# Simple Agent Prompt".to_string())
        );

        std::fs::remove_file(&test_file).ok();
    }
}
