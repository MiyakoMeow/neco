//! NeoCo Core Library
//!
//! This crate provides the core type system for the NeoCo project,
//! including strong type IDs, event system, error types, and domain traits.

#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::inherent_to_string_shadow_display)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::uninlined_format_args)]
#![allow(unused_crate_dependencies)]

pub mod errors;
pub mod events;
pub mod ids;
pub mod kernel;
pub mod messages;
pub mod prompt;
pub mod security;
pub mod tool;
pub mod traits;

pub use errors::*;
pub use ids::*;
// pub use kernel::*;
pub use events::{
    Agent, AgentEvent, AgentState, Event, EventFilter, EventPublisher, EventSubscriber,
    EventTypeFilter, Failed, FailureReason, SessionEvent, SessionType, SimpleEventPublisher, State,
    SystemEvent, Waiting, WaitingReason,
};
pub use messages::*;
pub use prompt::*;
pub use security::{SecurityContext, SecurityManager, SecurityResult};
pub use tool::{
    ResourceLevel, ToolCapabilities, ToolCategory, ToolContext, ToolDefinition, ToolExecutor,
    ToolOutput, ToolRegistry, ToolResult,
};
pub use traits::*;
