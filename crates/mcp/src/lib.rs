//! neoco-mcp
//!
//! MCP (Model Context Protocol) client implementation for `NeoCo`.

#![allow(unused_crate_dependencies)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use rmcp::{
    ClientHandler, Peer, RoleClient, ServiceExt,
    model::{ClientCapabilities, Implementation},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol_str::SmolStr;
use thiserror::Error;
use tokio::sync::{RwLock, broadcast};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

pub use neoco_config::McpServerConfig;
pub use neoco_core::ToolCapabilities;
pub use neoco_core::ToolCategory;
pub use neoco_core::ToolContext;
pub use neoco_core::ToolDefinition;
pub use neoco_core::ToolError;
pub use neoco_core::ToolExecutor;
pub use neoco_core::ToolOutput;
pub use neoco_core::ToolRegistry;
pub use neoco_core::ToolResult;
pub use neoco_core::ids::ToolId;

/// Default connection pool size.
const DEFAULT_POOL_SIZE: usize = 1;
/// Default heartbeat interval in seconds.
const DEFAULT_HEARTBEAT_SECS: u64 = 30;
/// Default maximum reconnection attempts.
const DEFAULT_RECONNECT_MAX_ATTEMPTS: u32 = 3;
/// Default initial reconnection delay in milliseconds.
const DEFAULT_RECONNECT_INITIAL_DELAY_MS: u64 = 1000;
/// Default maximum reconnection delay in milliseconds.
const DEFAULT_RECONNECT_MAX_DELAY_MS: u64 = 4000;
/// Default reconnection backoff multiplier.
const DEFAULT_RECONNECT_BACKOFF_MULTIPLIER: f64 = 2.0;

/// Errors that can occur when working with MCP servers.
#[derive(Debug, Clone, Error)]
pub enum McpError {
    /// Connection failed.
    #[error("连接失败: {0}")]
    ConnectionFailed(String),

    /// Tool call failed.
    #[error("工具调用失败: {0}")]
    ToolCallFailed(String),

    /// Server error.
    #[error("服务器错误: {0}")]
    ServerError(String),

    /// Operation timed out.
    #[error("超时")]
    Timeout,

    /// Protocol error.
    #[error("协议错误: {0}")]
    ProtocolError(String),

    /// Authentication failed.
    #[error("认证失败")]
    AuthenticationFailed,
}

impl McpError {
    /// Checks if the error is retryable.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout | Self::ConnectionFailed(_))
    }
}

/// Reason for disconnection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    /// Connection was closed by remote.
    RemoteClosed,
    /// Transport error occurred.
    TransportError,
    /// Connection timed out.
    Timeout,
    /// Maximum retry attempts exceeded.
    MaxRetriesExceeded,
}

/// Connection lifecycle events.
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// Connected to server.
    Connected {
        /// Server name.
        server: String,
    },
    /// Disconnected from server.
    Disconnected {
        /// Server name.
        server: String,
        /// Disconnect reason.
        reason: DisconnectReason,
    },
    /// Reconnecting to server.
    Reconnecting {
        /// Server name.
        server: String,
        /// Reconnection attempt number.
        attempt: u32,
    },
    /// Error occurred.
    Error {
        /// Server name.
        server: String,
        /// Error details.
        error: McpError,
    },
}

/// Status of an MCP server connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpServerStatus {
    /// Server is disconnected.
    Disconnected,
    /// Server is connecting.
    Connecting,
    /// Server is connected.
    Connected,
    /// Server is reconnecting.
    Reconnecting,
    /// Server encountered an error.
    Error,
}

impl std::fmt::Display for McpServerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected => write!(f, "Connected"),
            Self::Reconnecting => write!(f, "Reconnecting"),
            Self::Error => write!(f, "Error"),
        }
    }
}

/// Tool definition from an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Name of the tool.
    pub name: String,
    /// Description of the tool.
    pub description: String,
    /// JSON schema for the tool's input.
    pub input_schema: serde_json::Value,
}

/// Configuration for reconnection behavior.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of reconnection attempts.
    pub max_attempts: u32,
    /// Initial delay in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum delay in milliseconds.
    pub max_delay_ms: u64,
    /// Backoff multiplier.
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_RECONNECT_MAX_ATTEMPTS,
            initial_delay_ms: DEFAULT_RECONNECT_INITIAL_DELAY_MS,
            max_delay_ms: DEFAULT_RECONNECT_MAX_DELAY_MS,
            backoff_multiplier: DEFAULT_RECONNECT_BACKOFF_MULTIPLIER,
        }
    }
}

/// Configuration for MCP server connections.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Pool size for connections.
    pub pool_size: usize,
    /// Heartbeat interval in seconds.
    pub heartbeat_secs: u64,
    /// Reconnection configuration.
    pub reconnect: ReconnectConfig,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            pool_size: DEFAULT_POOL_SIZE,
            heartbeat_secs: DEFAULT_HEARTBEAT_SECS,
            reconnect: ReconnectConfig::default(),
        }
    }
}

/// Connection result type containing peer and running service info.
pub struct Connection {
    /// Peer for communicating with the MCP server.
    pub peer: Peer<RoleClient>,
}

impl Connection {
    /// Creates a new connection.
    #[must_use]
    pub fn new(peer: Peer<RoleClient>) -> Self {
        Self { peer }
    }
}

/// Connects to an MCP server using stdio transport.
///
/// # Errors
///
/// Returns an error if connection fails.
pub async fn connect_stdio<S: std::hash::BuildHasher + Default>(
    command: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String, S>,
) -> Result<Connection, McpError> {
    use rmcp::transport::TokioChildProcess;

    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&args);

    for (key, value) in env {
        cmd.env(&key, &value);
    }

    let transport = TokioChildProcess::new(cmd).map_err(|e| {
        McpError::ConnectionFailed(format!("Failed to create stdio transport: {e}"))
    })?;

    let running: rmcp::service::RunningService<RoleClient, McpClientHandler> = McpClientHandler
        .serve(transport)
        .await
        .map_err(|e| McpError::ConnectionFailed(format!("Failed to serve client: {e}")))?;

    let peer = running.peer().clone();

    debug!("Stdio connection established");
    Ok(Connection::new(peer))
}

/// Connects to an MCP server using HTTP transport.
///
/// # Errors
///
/// Returns an error if connection fails.
pub async fn connect_http<S: std::hash::BuildHasher + Default>(
    url: &str,
    bearer_token: Option<&str>,
    headers: &std::collections::HashMap<String, String, S>,
) -> Result<Connection, McpError> {
    use http::{HeaderName, HeaderValue};
    use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;

    let mut config = StreamableHttpClientTransportConfig::with_uri(url);

    if let Some(token) = bearer_token {
        config = config.auth_header(token);
    }

    if !headers.is_empty() {
        let mut custom_headers = std::collections::HashMap::new();
        for (key, value) in headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::try_from(key.as_str()),
                HeaderValue::from_str(value),
            ) {
                custom_headers.insert(name, val);
            }
        }
        config = config.custom_headers(custom_headers);
    }

    let transport = rmcp::transport::StreamableHttpClientTransport::from_config(config);

    let running: rmcp::service::RunningService<RoleClient, McpClientHandler> = McpClientHandler
        .serve(transport)
        .await
        .map_err(|e| McpError::ConnectionFailed(format!("Failed to serve client: {e}")))?;

    let peer = running.peer().clone();

    debug!("HTTP connection established");
    Ok(Connection::new(peer))
}

/// Fetches tools from an MCP server.
///
/// # Errors
///
/// Returns an error if fetching tools fails.
async fn fetch_tools(connection: &Connection) -> Result<Vec<McpTool>, McpError> {
    let result = connection
        .peer
        .list_all_tools()
        .await
        .map_err(|e| McpError::ToolCallFailed(format!("Failed to list tools: {e}")))?;

    let tools: Vec<McpTool> = result
        .into_iter()
        .map(|tool| McpTool {
            name: tool.name.to_string(),
            description: tool.description.unwrap_or_default().to_string(),
            input_schema: Value::Object((*tool.input_schema).clone()),
        })
        .collect();

    Ok(tools)
}

/// Represents a connection to an MCP server.
pub struct McpConnection {
    /// Name of the server.
    pub name: String,
    /// Server configuration.
    pub config: McpServerConfig,
    /// Current connection status.
    pub status: McpServerStatus,
    /// Available tools from the server.
    pub tools: Vec<McpTool>,
    /// Peer for communicating with the MCP server.
    pub peer: Option<Peer<RoleClient>>,
}

impl McpConnection {
    /// Creates a new MCP connection.
    #[must_use]
    pub fn new(name: String, config: McpServerConfig) -> Self {
        Self {
            name,
            config,
            status: McpServerStatus::Disconnected,
            tools: Vec::new(),
            peer: None,
        }
    }
}

/// Client handler for MCP server communication.
struct McpClientHandler;

#[async_trait]
impl ClientHandler for McpClientHandler {
    fn get_info(&self) -> rmcp::model::InitializeRequestParams {
        rmcp::model::InitializeRequestParams::new(
            ClientCapabilities::default(),
            Implementation::new("neoco".to_string(), "0.1.0".to_string()),
        )
    }
}

/// Manager for MCP server connections.
pub struct McpManager {
    /// Active connections to MCP servers.
    connections: DashMap<String, Arc<RwLock<McpConnection>>>,
    /// Server configurations.
    config: HashMap<String, McpServerConfig>,
    /// Connection configuration.
    connection_config: ConnectionConfig,
    /// Event publisher for connection lifecycle events.
    event_sender: broadcast::Sender<ConnectionEvent>,
}

impl McpManager {
    /// Creates a new MCP manager.
    #[must_use]
    pub fn new(config: HashMap<String, McpServerConfig>) -> Self {
        let (sender, _) = broadcast::channel(100);
        Self {
            connections: DashMap::new(),
            config,
            connection_config: ConnectionConfig::default(),
            event_sender: sender,
        }
    }

    /// Sets custom connection configuration.
    #[must_use]
    pub fn with_connection_config(mut self, connection_config: ConnectionConfig) -> Self {
        self.connection_config = connection_config;
        self
    }

    /// Gets a connection by name.
    #[must_use]
    pub fn connection(&self, name: &str) -> Option<Arc<RwLock<McpConnection>>> {
        self.connections.get(name).map(|e| Arc::clone(&e))
    }

    /// Lists all configured servers.
    #[must_use]
    pub fn servers(&self) -> Vec<String> {
        self.config.keys().cloned().collect()
    }

    /// Subscribe to connection events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<ConnectionEvent> {
        self.event_sender.subscribe()
    }

    /// Connects to an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server is not found or connection fails.
    pub async fn connect(&self, name: &str) -> Result<Arc<RwLock<McpConnection>>, McpError> {
        let config = self
            .config
            .get(name)
            .ok_or_else(|| McpError::ConnectionFailed(format!("Server '{name}' not found")))?
            .clone();

        let conn = Arc::new(RwLock::new(McpConnection::new(
            name.to_string(),
            config.clone(),
        )));

        {
            let mut conn_guard = conn.write().await;
            conn_guard.status = McpServerStatus::Connecting;
        }

        let connection = match self.establish_connection(&config).await {
            Ok(c) => c,
            Err(e) => {
                let _ = self.event_sender.send(ConnectionEvent::Error {
                    server: name.to_string(),
                    error: e.clone(),
                });
                return Err(e);
            },
        };
        let tools = fetch_tools(&connection).await?;

        {
            let mut conn_guard = conn.write().await;
            conn_guard.status = McpServerStatus::Connected;
            conn_guard.peer = Some(connection.peer);
            conn_guard.tools = tools;
        }

        self.connections.insert(name.to_string(), Arc::clone(&conn));

        let _ = self.event_sender.send(ConnectionEvent::Connected {
            server: name.to_string(),
        });

        info!("MCP server '{}' connected successfully", name);
        Ok(conn)
    }

    /// Establishes connection based on transport type.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    async fn establish_connection(&self, config: &McpServerConfig) -> Result<Connection, McpError> {
        let reconnect_config = &self.connection_config.reconnect;
        let mut attempt = 0;
        let mut delay_ms = reconnect_config.initial_delay_ms;

        let bearer_token = config
            .transport
            .as_ref()
            .and_then(|t| t.bearer_token.as_ref())
            .map(neoco_config::SecretString::as_str);

        let headers: HashMap<String, String> = config
            .transport
            .as_ref()
            .map(|t| {
                t.headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        loop {
            attempt += 1;

            let result: Result<Connection, McpError> =
                if let Some(ref transport_config) = config.transport {
                    match transport_config.transport_type {
                        neoco_config::McpTransportType::Stdio => {
                            let command = transport_config
                                .command
                                .clone()
                                .or_else(|| config.command.clone());
                            let args = transport_config
                                .args
                                .clone()
                                .or_else(|| config.args.clone());
                            if let (Some(cmd), Some(a)) = (command, args) {
                                let env: HashMap<String, String> = config
                                    .env
                                    .iter()
                                    .map(|(k, v)| (k.to_string(), v.to_string()))
                                    .collect();
                                connect_stdio(
                                    cmd.to_string(),
                                    a.iter().map(SmolStr::to_string).collect(),
                                    env,
                                )
                                .await
                            } else {
                                Err(McpError::ConnectionFailed(
                                    "No command/args found for stdio transport".to_string(),
                                ))
                            }
                        },
                        neoco_config::McpTransportType::Http => {
                            let url = transport_config.url.clone().or_else(|| config.url.clone());
                            if let Some(u) = url {
                                connect_http(u.as_str(), bearer_token, &headers).await
                            } else {
                                Err(McpError::ConnectionFailed(
                                    "No URL found for HTTP transport".to_string(),
                                ))
                            }
                        },
                    }
                } else if let Some(ref url) = config.url {
                    connect_http(url.as_str(), bearer_token, &headers).await
                } else if let (Some(command), Some(args)) = (&config.command, &config.args) {
                    let env: HashMap<String, String> = config
                        .env
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                    connect_stdio(
                        command.to_string(),
                        args.iter().map(SmolStr::to_string).collect(),
                        env,
                    )
                    .await
                } else {
                    return Err(McpError::ConnectionFailed(
                        "No valid transport configuration found".to_string(),
                    ));
                };

            match result {
                Ok(connection) => return Ok(connection),
                Err(e) => {
                    if attempt >= reconnect_config.max_attempts {
                        return Err(e);
                    }

                    warn!(
                        "Connection attempt {} failed: {}, retrying in {}ms",
                        attempt, e, delay_ms
                    );

                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                    delay_ms = (((delay_ms as f64) * reconnect_config.backoff_multiplier).ceil()
                        as u64)
                        .min(reconnect_config.max_delay_ms);
                },
            }
        }
    }

    /// Starts heartbeat for a connection.
    pub fn start_heartbeat(&self, name: String) {
        let heartbeat_interval = Duration::from_secs(self.connection_config.heartbeat_secs);

        let connections = Arc::new(self.connections.clone());

        tokio::spawn(async move {
            let mut ticker = interval(heartbeat_interval);

            loop {
                ticker.tick().await;

                let conn = {
                    let guard = connections.get(&name);
                    guard.map(|c| Arc::clone(&c))
                };

                let Some(conn) = conn else {
                    break;
                };

                let should_reconnect = {
                    let conn_guard = conn.write().await;

                    // 1. 检查连接状态
                    if conn_guard.status != McpServerStatus::Connected {
                        return;
                    }

                    // 2. 尝试发送 ping 请求来检测连接是否真正可用
                    if let Some(ref peer) = conn_guard.peer {
                        let ping_result = tokio::time::timeout(
                            Duration::from_secs(10),
                            peer.send_request(rmcp::model::ClientRequest::PingRequest(
                                rmcp::model::PingRequest::default(),
                            )),
                        )
                        .await;

                        match ping_result {
                            Ok(Ok(_)) => {
                                // Ping 成功，连接正常
                                false
                            },
                            Ok(Err(e)) => {
                                // Ping 返回错误，连接可能有问题
                                warn!(
                                    "Ping failed for server '{}': {}, connection may be lost",
                                    name, e
                                );
                                true
                            },
                            Err(_) => {
                                // Ping 超时，连接可能已断开
                                warn!("Ping timeout for server '{}', connection may be lost", name);
                                true
                            },
                        }
                    } else {
                        // 没有 peer 对象，需要重连
                        true
                    }
                };

                if should_reconnect {
                    let mut conn_guard = conn.write().await;
                    conn_guard.status = McpServerStatus::Reconnecting;

                    let config = conn_guard.config.clone();
                    drop(conn_guard);

                    let reconnect_bearer_token = config
                        .transport
                        .as_ref()
                        .and_then(|t| t.bearer_token.as_ref())
                        .map(neoco_config::SecretString::as_str);

                    let reconnect_headers: HashMap<String, String> = config
                        .transport
                        .as_ref()
                        .map(|t| {
                            t.headers
                                .iter()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();

                    let reconnect_result = if let Some(url) = &config.url {
                        connect_http(url.as_str(), reconnect_bearer_token, &reconnect_headers).await
                    } else if let (Some(command), Some(args)) = (&config.command, &config.args) {
                        let env: HashMap<String, String> = config
                            .env
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect();
                        connect_stdio(
                            command.to_string(),
                            args.iter().map(SmolStr::to_string).collect(),
                            env,
                        )
                        .await
                    } else {
                        Err(McpError::ConnectionFailed(
                            "No valid transport config".to_string(),
                        ))
                    };

                    let mut conn_guard = conn.write().await;
                    match reconnect_result {
                        Ok(connection) => {
                            conn_guard.peer = Some(connection.peer);
                            conn_guard.status = McpServerStatus::Connected;
                            info!("Successfully reconnected to server '{}'", name);
                        },
                        Err(e) => {
                            conn_guard.status = McpServerStatus::Error;
                            error!("Failed to reconnect to server '{}': {}", name, e);
                        },
                    }
                }
            }

            debug!("Heartbeat stopped for server '{}'", name);
        });
    }

    /// Disconnects from an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if disconnection fails.
    pub async fn disconnect(&self, name: &str) -> Result<(), McpError> {
        if let Some(conn) = self.connections.get(name) {
            let mut conn_guard = conn.write().await;
            conn_guard.status = McpServerStatus::Disconnected;
            conn_guard.tools.clear();
            conn_guard.peer = None;
        }
        self.connections.remove(name);
        Ok(())
    }

    /// Reconnects to an MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if reconnection fails.
    pub async fn reconnect(&self, name: &str) -> Result<Arc<RwLock<McpConnection>>, McpError> {
        self.disconnect(name).await?;
        self.connect(name).await
    }
}

/// Wrapper for MCP tools to implement the `ToolExecutor` trait.
pub struct McpToolWrapper {
    /// Name of the MCP server.
    server_name: String,
    /// The MCP tool.
    tool: McpTool,
    /// Reference to the MCP manager.
    manager: Arc<McpManager>,
    /// Cached tool definition.
    definition: ToolDefinition,
}

impl McpToolWrapper {
    /// Creates a new MCP tool wrapper.
    ///
    /// # Panics
    ///
    /// Panics if the tool ID cannot be constructed from the server name and tool name.
    #[must_use]
    pub fn new(server_name: String, tool: McpTool, manager: Arc<McpManager>) -> Self {
        let definition = ToolDefinition {
            id: ToolId::from_string(&format!("mcp::{server_name}::{}", tool.name)).unwrap(),
            description: tool.description.clone(),
            schema: tool.input_schema.clone(),
            capabilities: ToolCapabilities::default(),
            timeout: Duration::from_secs(60),
            category: ToolCategory::Common,
            prompt_component: None,
        };
        Self {
            server_name,
            tool,
            manager,
            definition,
        }
    }

    /// Executes the MCP tool.
    ///
    /// # Errors
    ///
    /// Returns an error if execution fails.
    async fn do_execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        let conn = self.manager.connection(&self.server_name).ok_or_else(|| {
            ToolError::ExecutionFailed(format!("Server '{}' not connected", self.server_name))
        })?;

        let conn_guard = conn.read().await;
        if conn_guard.status != McpServerStatus::Connected {
            return Err(ToolError::ExecutionFailed(format!(
                "Server '{}' is not connected (status: {})",
                self.server_name, conn_guard.status
            )));
        }

        let peer = conn_guard.peer.as_ref().ok_or_else(|| {
            ToolError::ExecutionFailed(format!("Server '{}' peer not available", self.server_name))
        })?;

        let arguments = args.as_object().cloned();
        let mut request =
            rmcp::model::CallToolRequestParams::new(Cow::Owned(self.tool.name.clone()));
        if let Some(args_obj) = arguments {
            request = request.with_arguments(args_obj);
        }

        let result = peer
            .call_tool(request)
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Tool call failed: {e}")))?;

        let is_error = result.is_error.unwrap_or(false);
        let content = result.content;

        let output = if content.is_empty() {
            ToolOutput::Empty
        } else {
            let text_content: Vec<String> = content
                .into_iter()
                .filter_map(|c| c.as_text().map(|t| t.text.clone()))
                .collect();

            if text_content.is_empty() {
                ToolOutput::Empty
            } else {
                ToolOutput::Text(text_content.join("\n"))
            }
        };

        Ok(ToolResult {
            output,
            is_error,
            prompt_component: None,
        })
    }
}

#[async_trait]
impl ToolExecutor for McpToolWrapper {
    fn definition(&self) -> &ToolDefinition {
        &self.definition
    }

    async fn execute(
        &self,
        _context: &ToolContext,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        self.do_execute(args).await
    }
}

/// Registers MCP tools from a server to the tool registry.
///
/// # Errors
///
/// Returns an error if registration fails.
pub async fn register_mcp_tools(
    manager: &Arc<McpManager>,
    registry: &dyn ToolRegistry,
    server_name: &str,
) -> Result<usize, McpError> {
    let conn = manager.connect(server_name).await?;

    let tools = {
        let conn_guard = conn.read().await;
        conn_guard.tools.clone()
    };

    let manager = Arc::clone(manager);
    for tool in &tools {
        let wrapper =
            McpToolWrapper::new(server_name.to_string(), tool.clone(), Arc::clone(&manager));
        registry.register(Arc::new(wrapper)).await;
    }

    info!(
        "Registered {} tools for MCP server '{}'",
        tools.len(),
        server_name
    );

    Ok(tools.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test as _;

    #[test]
    fn test_mcp_error_is_retryable() {
        assert!(McpError::Timeout.is_retryable());
        assert!(McpError::ConnectionFailed("test".to_string()).is_retryable());
        assert!(!McpError::ToolCallFailed("test".to_string()).is_retryable());
        assert!(!McpError::ServerError("test".to_string()).is_retryable());
        assert!(!McpError::ProtocolError("test".to_string()).is_retryable());
        assert!(!McpError::AuthenticationFailed.is_retryable());
    }

    #[test]
    fn test_mcp_server_status_display() {
        assert_eq!(McpServerStatus::Disconnected.to_string(), "Disconnected");
        assert_eq!(McpServerStatus::Connecting.to_string(), "Connecting");
        assert_eq!(McpServerStatus::Connected.to_string(), "Connected");
        assert_eq!(McpServerStatus::Reconnecting.to_string(), "Reconnecting");
        assert_eq!(McpServerStatus::Error.to_string(), "Error");
    }

    #[test]
    fn test_reconnect_config_default() {
        let config = ReconnectConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 4000);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_connection_config_default() {
        let config = ConnectionConfig::default();
        assert_eq!(config.pool_size, 1);
        assert_eq!(config.heartbeat_secs, 30);
    }

    #[test]
    fn test_mcp_manager_new() {
        let config = HashMap::new();
        let manager = McpManager::new(config);
        assert!(manager.servers().is_empty());
    }

    #[tokio::test]
    async fn test_mcp_manager_with_connection_config() {
        let config = HashMap::new();
        let connection_config = ConnectionConfig {
            pool_size: 2,
            heartbeat_secs: 60,
            reconnect: ReconnectConfig {
                max_attempts: 5,
                ..Default::default()
            },
        };
        let manager = McpManager::new(config).with_connection_config(connection_config);
        assert!(manager.connection_config.pool_size == 2);
        assert!(manager.connection_config.heartbeat_secs == 60);
        assert!(manager.connection_config.reconnect.max_attempts == 5);
    }

    #[tokio::test]
    async fn test_mcp_connection_new() {
        let config = McpServerConfig::default();
        let conn = McpConnection::new("test".to_string(), config);
        assert_eq!(conn.name, "test");
        assert_eq!(conn.status, McpServerStatus::Disconnected);
        assert!(conn.tools.is_empty());
        assert!(conn.peer.is_none());
    }

    #[test]
    fn test_tool_id_format() {
        let server_name = "github";
        let tool_name = "create_issue";
        let id = ToolId::from_string(&format!("mcp::{server_name}::{tool_name}")).unwrap();
        assert_eq!(id.to_string(), format!("mcp::{server_name}::{tool_name}"));
    }
}
