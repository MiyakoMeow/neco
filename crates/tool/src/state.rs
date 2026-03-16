//! Tool execution state machine with type-state pattern.
//!
//! This module provides compile-time guarantees for valid state transitions.

use std::fmt;
use std::marker::PhantomData;

/// Sealed module for preventing trait implementation outside this crate.
mod sealed {
    /// Sealed trait for preventing implementation outside this crate.
    pub trait Sealed {}
}

/// Base trait for all execution states (compile-time).
pub trait State: sealed::Sealed + fmt::Debug + Clone + Default {
    /// Whether this state is terminal.
    const IS_TERMINAL: bool;
    /// Whether tool can be executed from this state.
    const CAN_EXECUTE: bool;
}

/// Idle state - waiting for execution.
#[derive(Debug, Clone, Copy, Default)]
pub struct Idle;
impl sealed::Sealed for Idle {}
impl State for Idle {
    const IS_TERMINAL: bool = false;
    const CAN_EXECUTE: bool = true;
}

/// Resolving state - parsing tool and arguments.
#[derive(Debug, Clone, Copy, Default)]
pub struct Resolving;
impl sealed::Sealed for Resolving {}
impl State for Resolving {
    const IS_TERMINAL: bool = false;
    const CAN_EXECUTE: bool = false;
}

/// Validating state - validating parameters and permissions.
#[derive(Debug, Clone, Copy, Default)]
pub struct Validating;
impl sealed::Sealed for Validating {}
impl State for Validating {
    const IS_TERMINAL: bool = false;
    const CAN_EXECUTE: bool = false;
}

/// Executing state - executing tool logic.
#[derive(Debug, Clone, Copy, Default)]
pub struct Executing;
impl sealed::Sealed for Executing {}
impl State for Executing {
    const IS_TERMINAL: bool = false;
    const CAN_EXECUTE: bool = false;
}

/// Processing state - processing execution result.
#[derive(Debug, Clone, Copy, Default)]
pub struct Processing;
impl sealed::Sealed for Processing {}
impl State for Processing {
    const IS_TERMINAL: bool = false;
    const CAN_EXECUTE: bool = false;
}

/// Completed state - execution finished successfully (terminal).
#[derive(Debug, Clone, Copy, Default)]
pub struct Completed;
impl sealed::Sealed for Completed {}
impl State for Completed {
    const IS_TERMINAL: bool = true;
    const CAN_EXECUTE: bool = false;
}

/// Failed state - execution failed (terminal).
#[derive(Clone)]
pub struct Failed<E = std::convert::Infallible> {
    /// Error associated with the failure.
    error: E,
}

impl<E> Failed<E> {
    /// Create a new Failed state with an error.
    pub fn new(error: E) -> Self {
        Self { error }
    }
    /// Get the error associated with this failure.
    pub fn error(&self) -> &E {
        &self.error
    }
}

impl<E: fmt::Debug> fmt::Debug for Failed<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Failed")
            .field("error", &self.error)
            .finish()
    }
}

impl<E: Default> Default for Failed<E> {
    fn default() -> Self {
        Self {
            error: E::default(),
        }
    }
}

impl<E> sealed::Sealed for Failed<E> {}

impl<E: fmt::Debug + Clone + Default> State for Failed<E> {
    const IS_TERMINAL: bool = true;
    const CAN_EXECUTE: bool = false;
}

/// Type-state tool executor.
/// `S` is the current execution state.
/// `E` is the error type.
#[derive(Debug)]
pub struct ToolExecutor<S, E = std::convert::Infallible> {
    /// State marker.
    _state: PhantomData<S>,
    /// Error marker.
    _error: PhantomData<E>,
}

impl<S: State, E> ToolExecutor<S, E> {
    /// Get the state name as a string.
    pub fn state_name() -> &'static str {
        match std::any::type_name::<S>() {
            s if s.ends_with("Idle") => "Idle",
            s if s.ends_with("Resolving") => "Resolving",
            s if s.ends_with("Validating") => "Validating",
            s if s.ends_with("Executing") => "Executing",
            s if s.ends_with("Processing") => "Processing",
            s if s.ends_with("Completed") => "Completed",
            s if s.ends_with("Failed") => "Failed",
            _ => "Unknown",
        }
    }
}

impl<E> ToolExecutor<Idle, E> {
    /// Create a new tool executor in Idle state.
    pub fn new() -> Self {
        Self {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
    /// Transition from Idle to Resolving.
    pub fn start(self) -> ToolExecutor<Resolving, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> ToolExecutor<Resolving, E> {
    /// Transition from Resolving to Validating.
    pub fn validate(self) -> ToolExecutor<Validating, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> ToolExecutor<Validating, E> {
    /// Transition from Validating to Executing.
    pub fn execute(self) -> ToolExecutor<Executing, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> ToolExecutor<Executing, E> {
    /// Transition from Executing to Processing.
    pub fn process(self) -> ToolExecutor<Processing, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> ToolExecutor<Processing, E> {
    /// Transition from Processing to Completed (success path).
    pub fn complete(self) -> ToolExecutor<Completed, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
    /// Transition from Processing to Failed (error path).
    pub fn fail(self, _error: E) -> ToolExecutor<Failed<E>, E> {
        ToolExecutor {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> ToolExecutor<Completed, E> {
    /// Create a new Completed executor.
    pub fn new() -> Self {
        Self {
            _state: PhantomData,
            _error: PhantomData,
        }
    }
}

impl<E> Default for ToolExecutor<Idle, E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for idle executor.
pub type IdleExecutor<E = std::convert::Infallible> = ToolExecutor<Idle, E>;
/// Type alias for completed executor.
pub type CompletedExecutor<E = std::convert::Infallible> = ToolExecutor<Completed, E>;
/// Type alias for failed executor.
pub type FailedExecutor<E> = ToolExecutor<Failed<E>, E>;

/// Legacy-compatible enum for runtime state checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionState {
    /// Idle state.
    #[default]
    Idle,
    /// Resolving state.
    Resolving,
    /// Validating state.
    Validating,
    /// Executing state.
    Executing,
    /// Processing state.
    Processing,
    /// Completed state.
    Completed,
    /// Failed state.
    Failed,
}

impl ExecutionState {
    /// Check if state is terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
    /// Check if tool can be executed from this state.
    #[must_use]
    pub fn can_execute(&self) -> bool {
        matches!(self, Self::Idle)
    }
    /// Get the next state on success path.
    #[must_use]
    pub fn next_success(&self) -> Self {
        match self {
            Self::Idle => Self::Resolving,
            Self::Resolving => Self::Validating,
            Self::Validating => Self::Executing,
            Self::Executing => Self::Processing,
            Self::Processing | Self::Completed | Self::Failed => Self::Completed,
        }
    }
    /// Get the next state on failure path.
    #[must_use]
    pub fn next_failure(&self) -> Self {
        match self {
            Self::Completed => Self::Completed,
            Self::Idle
            | Self::Resolving
            | Self::Validating
            | Self::Executing
            | Self::Processing
            | Self::Failed => Self::Failed,
        }
    }
}

impl fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Resolving => write!(f, "Resolving"),
            Self::Validating => write!(f, "Validating"),
            Self::Executing => write!(f, "Executing"),
            Self::Processing => write!(f, "Processing"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_successful_transition_path() {
        let executor = ToolExecutor::<Idle, std::convert::Infallible>::new();
        let executor = executor.start();
        assert_eq!(ToolExecutor::<Resolving>::state_name(), "Resolving");
        let executor = executor.validate();
        let executor = executor.execute();
        let executor = executor.process();
        let _executor = executor.complete();
        assert_eq!(ToolExecutor::<Completed>::state_name(), "Completed");
        assert!(<Completed as State>::IS_TERMINAL);
    }

    #[test]
    fn test_failure_transition() {
        let executor = ToolExecutor::<Idle, String>::new();
        let executor = executor.start();
        let executor = executor.validate();
        let executor = executor.execute();
        let executor = executor.process();
        let failed = executor.fail("error".to_string());
        assert!(<Failed<String> as State>::IS_TERMINAL);
        let _ = failed;
    }

    #[test]
    fn test_idle_can_execute() {
        assert!(<Idle as State>::CAN_EXECUTE);
    }

    #[test]
    fn test_execution_state_enum() {
        let state = ExecutionState::Idle;
        assert!(state.can_execute());
        assert_eq!(state.next_success(), ExecutionState::Resolving);
        let state = state
            .next_success()
            .next_success()
            .next_success()
            .next_success()
            .next_success();
        assert_eq!(state, ExecutionState::Completed);
        assert!(state.is_terminal());
    }
}
