//! Event system for event-driven architecture.
//!
//! This module provides both type-safe state machines (compile-time) and
//! backwards-compatible enums (runtime) for serialization.

use crate::ids::{AgentUlid, SessionUlid, ToolId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

/// Sealed module for preventing trait implementation outside this crate.
mod sealed {
    /// Sealed trait for preventing implementation outside this crate.
    pub trait Sealed {}
}

/// Base trait for agent states (compile-time).
/// Base trait for agent states (compile-time).
pub trait State: sealed::Sealed + fmt::Debug + Clone + Default {
    /// Whether state is terminal (completed or failed).
    const IS_TERMINAL: bool;
    /// Whether state is waiting.
    const IS_WAITING: bool;
    /// Whether state is failed.
    const IS_FAILED: bool;
}

/// Agent is idle (waiting for work).
#[derive(Debug, Clone, Copy, Default)]
pub struct Idle;
impl sealed::Sealed for Idle {}
impl State for Idle {
    const IS_TERMINAL: bool = false;
    const IS_WAITING: bool = false;
    const IS_FAILED: bool = false;
}

/// Agent is actively running.
#[derive(Debug, Clone, Copy, Default)]
pub struct Running;
impl sealed::Sealed for Running {}
impl State for Running {
    const IS_TERMINAL: bool = false;
    const IS_WAITING: bool = false;
    const IS_FAILED: bool = false;
}

/// Waiting reason marker trait.
/// Marker trait for waiting reasons.
pub trait WaitReason: sealed::Sealed + fmt::Debug + Clone + Default {}

/// Waiting for tool call.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaitToolCall;
impl sealed::Sealed for WaitToolCall {}
impl WaitReason for WaitToolCall {}

/// Waiting for user input.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaitUserInput;
impl sealed::Sealed for WaitUserInput {}
impl WaitReason for WaitUserInput {}

/// Waiting for sub-agent.
#[derive(Debug, Clone, Copy, Default)]
pub struct WaitSubAgent;
impl sealed::Sealed for WaitSubAgent {}
impl WaitReason for WaitSubAgent {}

/// Agent is waiting for something.
#[derive(Debug, Clone)]
pub struct Waiting<W: WaitReason = WaitToolCall> {
    /// Waiting reason marker.
    _reason: PhantomData<W>,
}

impl<W: WaitReason> Waiting<W> {
    /// Create a new Agent in idle state.
    pub fn new() -> Self {
        Self {
            _reason: PhantomData,
        }
    }
}

impl<W: WaitReason> Default for Waiting<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: WaitReason> sealed::Sealed for Waiting<W> {}

impl<W: WaitReason> State for Waiting<W> {
    const IS_TERMINAL: bool = false;
    const IS_WAITING: bool = true;
    const IS_FAILED: bool = false;
}

/// Agent completed successfully (terminal).
#[derive(Debug, Clone, Copy, Default)]
pub struct Completed;
impl sealed::Sealed for Completed {}
impl State for Completed {
    const IS_TERMINAL: bool = true;
    const IS_WAITING: bool = false;
    const IS_FAILED: bool = false;
}

/// Failure reason marker trait.
/// Marker trait for failure reasons.
pub trait FailReason: sealed::Sealed + fmt::Debug + Clone + Default {}

/// Generic error failure.
#[derive(Debug, Clone, Default)]
pub struct FailError {
    /// Error message.
    pub message: String,
}

impl FailError {
    /// Create a new FailError.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl sealed::Sealed for FailError {}
impl FailReason for FailError {}

/// Task cancelled.
#[derive(Debug, Clone, Copy, Default)]
pub struct FailCancelled;
impl sealed::Sealed for FailCancelled {}
impl FailReason for FailCancelled {}

/// Task timeout.
#[derive(Debug, Clone, Copy, Default)]
pub struct FailTimeout;
impl sealed::Sealed for FailTimeout {}
impl FailReason for FailTimeout {}

/// Agent failed (terminal).
#[derive(Debug, Clone)]
pub struct Failed<F: FailReason = FailError> {
    /// Failure reason.
    reason: F,
}

impl<F: FailReason> Failed<F> {
    /// Create a new Failed state.
    pub fn new(reason: F) -> Self {
        Self { reason }
    }

    /// Get the failure reason.
    pub fn reason(&self) -> &F {
        &self.reason
    }
}

impl<F: FailReason> Default for Failed<F>
where
    F: Default,
{
    fn default() -> Self {
        Self {
            reason: F::default(),
        }
    }
}

impl<F: FailReason> sealed::Sealed for Failed<F> {}

impl<F: FailReason> State for Failed<F> {
    const IS_TERMINAL: bool = true;
    const IS_WAITING: bool = false;
    const IS_FAILED: bool = true;
}

/// Type-state agent.
///
/// `S` is the current state.
/// `W` is the waiting reason (if waiting).
/// `F` is the failure reason (if failed).
#[derive(Debug)]
pub struct Agent<S = Idle, W = (), F = ()> {
    /// State marker.
    _state: PhantomData<S>,
    /// Wait marker.
    _wait: PhantomData<W>,
    /// Fail marker.
    _fail: PhantomData<F>,
}

impl Agent<Idle> {
    /// Create a new Agent in idle state.
    pub fn new() -> Self {
        Self {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Running state.
    pub fn run(self) -> Agent<Running> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Waiting state.
    pub fn wait<W: WaitReason>(self) -> Agent<Waiting<W>, W> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Completed state.
    pub fn complete(self) -> Agent<Completed> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Failed state.
    pub fn fail<F: FailReason>(self, _reason: F) -> Agent<Failed<F>, (), F> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }
}

impl Default for Agent<Idle> {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent<Running> {
    /// Transition to Waiting state.
    pub fn wait<W: WaitReason>(self) -> Agent<Waiting<W>, W> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Completed state.
    pub fn complete(self) -> Agent<Completed> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Failed state.
    pub fn fail<F: FailReason>(self, _reason: F) -> Agent<Failed<F>, (), F> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }
}

impl<W: WaitReason> Agent<Waiting<W>, W> {
    /// Resume from waiting to running.
    pub fn resume(self) -> Agent<Running> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Complete from waiting state.
    /// Complete from waiting.
    pub fn complete(self) -> Agent<Completed> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }

    /// Transition to Failed from Waiting.
    pub fn fail<F: FailReason>(self, _reason: F) -> Agent<Failed<F>, (), F> {
        Agent {
            _state: PhantomData,
            _wait: PhantomData,
            _fail: PhantomData,
        }
    }
}

// ============== Legacy-compatible enums for serialization ==============

/// Reason for waiting state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitingReason {
    /// Waiting for tool call.
    ToolCall,
    /// Waiting for user input.
    UserInput,
    /// Waiting for sub-agent.
    SubAgent,
}

/// Reason for failure state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureReason {
    /// Error failure.
    Error(String),
    /// Recoverable failure.
    Recoverable(String),
    /// Unrecoverable failure.
    Unrecoverable(String),
    /// Task cancelled.
    Cancelled,
    /// Task timeout.
    Timeout,
}

/// Agent state (runtime-compatible, for serialization).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    /// Agent is idle.
    Idle,
    /// Agent is currently running.
    Running,
    /// Agent is waiting for something.
    Waiting(WaitingReason),
    /// Agent completed successfully.
    Completed,
    /// Agent failed.
    Failed(FailureReason),
}

impl AgentState {
    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Running => "running",
            AgentState::Waiting(_) => "waiting",
            AgentState::Completed => "completed",
            AgentState::Failed(_) => "failed",
        }
    }

    /// Check if transition to the target state is valid.
    #[must_use]
    pub fn can_transition_to(&self, target: &AgentState) -> bool {
        match (self, target) {
            // Idle -> Running: Start agent execution
            (AgentState::Idle, AgentState::Running) => true,
            // Running -> Waiting: Agent waiting for something
            (AgentState::Running, AgentState::Waiting(_)) => true,
            // Running -> Completed: Agent finished successfully
            (AgentState::Running, AgentState::Completed) => true,
            // Running -> Failed: Agent encountered an error
            (AgentState::Running, AgentState::Failed(_)) => true,
            // Waiting -> Running: Resume from waiting
            (AgentState::Waiting(_), AgentState::Running) => true,
            // Waiting -> Completed: Complete while waiting
            (AgentState::Waiting(_), AgentState::Completed) => true,
            // Waiting -> Failed: Fail while waiting
            (AgentState::Waiting(_), AgentState::Failed(_)) => true,
            // Completed -> Completed: Re-complete (idempotent)
            (AgentState::Completed, AgentState::Completed) => true,
            // Failed -> Failed: Re-fail (idempotent)
            (AgentState::Failed(_), AgentState::Failed(_)) => true,
            // Any -> Idle: Reset to initial state
            (_, AgentState::Idle) => true,
            // All other transitions are invalid
            _ => false,
        }
    }

    /// Check if this state is terminal (completed or failed).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Completed | AgentState::Failed(_))
    }
}

// ============== Unified event type ==============

/// Unified event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// Session-level event.
    Session(SessionEvent),
    /// Agent-level event.
    Agent(AgentEvent),
    /// Tool-level event.
    Tool(ToolEvent),
    /// System-level event.
    System(SystemEvent),
}

/// Session domain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionEvent {
    /// Session was created.
    Created {
        /// Session ID.
        id: SessionUlid,
        /// Session type.
        session_type: SessionType,
    },
    /// Session was updated.
    Updated {
        /// Session ID.
        id: SessionUlid,
    },
    /// Session was deleted.
    Deleted {
        /// Session ID.
        id: SessionUlid,
    },
}

/// Session type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionType {
    /// Direct session with optional initial message.
    Direct {
        /// Initial message.
        initial_message: Option<String>,
    },
    /// TUI session.
    #[serde(alias = "repl")]
    Tui,
    /// Workflow session.
    Workflow {
        /// Workflow ID.
        workflow_id: String,
    },
}

/// Agent domain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// Agent was created.
    Created {
        /// Agent ID.
        id: AgentUlid,
        /// Parent agent ID (None for root).
        parent_ulid: Option<AgentUlid>,
    },
    /// Agent state changed.
    StateChanged {
        /// Agent ID.
        id: AgentUlid,
        /// Previous state.
        old: AgentState,
        /// New state.
        new: AgentState,
    },
    /// Message was added to agent.
    MessageAdded {
        /// Agent ID.
        id: AgentUlid,
        /// Message ID.
        message_id: crate::ids::MessageId,
    },
    /// Agent called a tool.
    ToolCalled {
        /// Agent ID.
        id: AgentUlid,
        /// Tool ID.
        tool_id: ToolId,
    },
    /// Tool execution completed.
    ToolResult {
        /// Agent ID.
        id: AgentUlid,
        /// Tool ID.
        tool_id: ToolId,
        /// Whether execution succeeded.
        success: bool,
    },
    /// Agent completed execution.
    Completed {
        /// Agent ID.
        id: AgentUlid,
        /// Output content.
        output: String,
    },
    /// Agent encountered an error.
    Error {
        /// Agent ID.
        id: AgentUlid,
        /// Error message.
        error: String,
    },
}

/// Tool domain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolEvent {
    /// Tool was registered.
    Registered {
        /// Tool ID.
        tool_id: ToolId,
    },
    /// Tool is executing.
    Executing {
        /// Tool ID.
        tool_id: ToolId,
        /// Agent calling the tool.
        agent_ulid: AgentUlid,
    },
    /// Tool execution completed.
    Executed {
        /// Tool ID.
        tool_id: ToolId,
        /// Agent that called the tool.
        agent_ulid: AgentUlid,
        /// Whether execution succeeded.
        success: bool,
    },
    /// Tool execution error.
    Error {
        /// Tool ID.
        tool_id: ToolId,
        /// Error message.
        error: String,
    },
}

/// System-level events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemEvent {
    /// System started up.
    Startup,
    /// System shut down.
    Shutdown,
    /// System error.
    Error {
        /// Error source.
        source: String,
        /// Error message.
        message: String,
    },
}

/// Event publisher trait.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish an event.
    async fn publish(&self, event: Event);
    /// Subscribe to events with filter.
    fn subscribe(&self, filter: EventFilter) -> Arc<dyn EventSubscriber>;
}

/// Event subscriber trait.
#[async_trait]
pub trait EventSubscriber: Send + Sync {
    /// Handle an event.
    async fn on_event(&self, event: Event);
    /// Check if subscriber matches event.
    fn matches(&self, event: &Event) -> bool;
}

/// Event filter for subscribers.
#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Filter by session ID.
    pub session_id: Option<SessionUlid>,
    /// Filter by agent ID.
    pub agent_id: Option<AgentUlid>,
    /// Filter by event types.
    pub event_types: Vec<EventTypeFilter>,
}

/// Event type filter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventTypeFilter {
    /// Session events.
    Session,
    /// Agent events.
    Agent,
    /// Tool events.
    Tool,
    /// System events.
    System,
}

impl EventFilter {
    /// Create a new empty filter.
    /// Create a new Agent in idle state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by session.
    pub fn with_session(mut self, id: SessionUlid) -> Self {
        self.session_id = Some(id);
        self
    }

    /// Filter by agent.
    pub fn with_agent(mut self, id: AgentUlid) -> Self {
        self.agent_id = Some(id);
        self
    }

    /// Filter by event type.
    pub fn with_event_type(mut self, event_type: EventTypeFilter) -> Self {
        self.event_types.push(event_type);
        self
    }

    /// Check if event matches this filter.
    pub fn matches(&self, event: &Event) -> bool {
        if !self.event_types.is_empty() {
            let type_matches = match event {
                Event::Session(_) => self.event_types.contains(&EventTypeFilter::Session),
                Event::Agent(_) => self.event_types.contains(&EventTypeFilter::Agent),
                Event::Tool(_) => self.event_types.contains(&EventTypeFilter::Tool),
                Event::System(_) => self.event_types.contains(&EventTypeFilter::System),
            };
            if !type_matches {
                return false;
            }
        }

        match event {
            Event::Session(e) => {
                if let Some(id) = &self.session_id {
                    match e {
                        SessionEvent::Created { id: sid, .. }
                        | SessionEvent::Updated { id: sid }
                        | SessionEvent::Deleted { id: sid } => {
                            if sid != id {
                                return false;
                            }
                        },
                    }
                }
            },
            Event::Agent(e) => {
                if let Some(id) = &self.agent_id {
                    let agent_id = match e {
                        AgentEvent::Created { id: aid, .. }
                        | AgentEvent::StateChanged { id: aid, .. }
                        | AgentEvent::MessageAdded { id: aid, .. }
                        | AgentEvent::ToolCalled { id: aid, .. }
                        | AgentEvent::ToolResult { id: aid, .. }
                        | AgentEvent::Completed { id: aid, .. }
                        | AgentEvent::Error { id: aid, .. } => aid,
                    };
                    if agent_id != id {
                        return false;
                    }
                }
            },
            Event::Tool(_) | Event::System(_) => {},
        }

        true
    }
}

/// Simple in-memory event publisher.
pub struct SimpleEventPublisher {
    /// Subscribers list.
    subscribers: std::sync::RwLock<Vec<Arc<dyn EventSubscriber>>>,
}

impl SimpleEventPublisher {
    /// Create a new publisher.
    /// Create a new Agent in idle state.
    pub fn new() -> Self {
        Self {
            subscribers: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// Add a subscriber.
    pub fn add_subscriber(&self, subscriber: Arc<dyn EventSubscriber>) {
        self.subscribers.write().unwrap().push(subscriber);
    }
}

impl Default for SimpleEventPublisher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventPublisher for SimpleEventPublisher {
    async fn publish(&self, event: Event) {
        let subscribers: Vec<_> = self.subscribers.read().unwrap().clone();
        for subscriber in subscribers {
            if subscriber.matches(&event) {
                subscriber.on_event(event.clone()).await;
            }
        }
    }

    fn subscribe(&self, filter: EventFilter) -> Arc<dyn EventSubscriber> {
        let subscriber = FilteredSubscriber::new(filter);
        let subscriber = Arc::new(subscriber);
        self.add_subscriber(subscriber.clone());
        subscriber
    }
}

/// Filtered subscriber wrapper.
struct FilteredSubscriber {
    /// Event filter.
    filter: EventFilter,
}

impl FilteredSubscriber {
    /// Creates a new FilteredSubscriber.
    fn new(filter: EventFilter) -> Self {
        Self { filter }
    }
}

#[async_trait]
impl EventSubscriber for FilteredSubscriber {
    async fn on_event(&self, _event: Event) {}

    fn matches(&self, event: &Event) -> bool {
        self.filter.matches(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_state_agent() {
        let agent: Agent<Idle> = Agent::new();

        let agent = agent.run();
        assert!(!<Running as State>::IS_TERMINAL);

        let agent = agent.wait::<WaitToolCall>();
        assert!(<Waiting<WaitToolCall> as State>::IS_WAITING);

        let _agent = agent.complete();
        assert!(<Completed as State>::IS_TERMINAL);
    }

    #[test]
    fn test_agent_state_enum() {
        assert_eq!(AgentState::Idle.as_str(), "idle");
        assert_eq!(AgentState::Running.as_str(), "running");
        assert_eq!(
            AgentState::Waiting(WaitingReason::ToolCall).as_str(),
            "waiting"
        );
        assert_eq!(AgentState::Completed.as_str(), "completed");
        assert_eq!(
            AgentState::Failed(FailureReason::Error(String::new())).as_str(),
            "failed"
        );
    }

    #[test]
    fn test_failed_state() {
        let failed: Failed<FailError> = Failed::new(FailError::new("test error"));
        assert!(<Failed<FailError> as State>::IS_TERMINAL);
        assert!(<Failed<FailError> as State>::IS_FAILED);
        assert_eq!(failed.reason().message, "test error");
    }

    #[test]
    fn test_event_filter_by_session() {
        let session = SessionUlid::new();
        let filter = EventFilter::new().with_session(session);

        let event = Event::Session(SessionEvent::Created {
            id: session,
            session_type: SessionType::Direct {
                initial_message: None,
            },
        });
        assert!(filter.matches(&event));

        let other_session = SessionUlid::new();
        let other_event = Event::Session(SessionEvent::Created {
            id: other_session,
            session_type: SessionType::Direct {
                initial_message: None,
            },
        });
        assert!(!filter.matches(&other_event));
    }

    #[test]
    fn test_simple_event_publisher() {
        let publisher = SimpleEventPublisher::new();
        let event = Event::System(SystemEvent::Startup);
        let subscribers: Vec<_> = publisher.subscribers.read().unwrap().clone();
        for subscriber in subscribers {
            if subscriber.matches(&event) {}
        }
    }
}
