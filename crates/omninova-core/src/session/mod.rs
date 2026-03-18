//! Session and message management module
//!
//! This module provides data models and storage operations for
//! conversation sessions and messages.

mod model;
mod store;

pub use model::{Message, MessageRole, NewMessage, NewSession, Session, SessionUpdate};
pub use store::{MessageStore, SessionStore, SessionStoreError};