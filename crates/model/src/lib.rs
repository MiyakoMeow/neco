#![allow(clippy::doc_markdown)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::unused_self)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::assigning_clones)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unnecessary_join)]
#![allow(clippy::map_unwrap_or)]
#![allow(unused_crate_dependencies)]

//! neoco-model - Model service module for NeoCo
//!
//! This module provides unified interfaces for interacting with various LLM providers,
//! including support for fallback, retry, and streaming.

// Suppress unused crate warning
use async_openai as _;

pub mod client;
pub mod error;
pub mod model_group;
pub mod providers;
pub mod registry;
pub mod stream;
pub mod tool_call_handler;
pub mod types;

pub use client::*;
pub use error::*;
pub use model_group::*;
pub use registry::*;
pub use stream::*;
pub use types::*;
