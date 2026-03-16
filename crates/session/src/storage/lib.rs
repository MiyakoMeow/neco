//! Storage module.

pub mod file;
pub mod memory;

pub use file::FileStorage;
pub use memory::InMemoryStorage;
