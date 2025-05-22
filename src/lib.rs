//! MongoDB Model ORM
//!
//! It includes CRUD operations, query building, and index management with support for:
//! - Field renaming
//! - Hidden/visible fields
//! - Timestamps
//! - Transactions
//! - Aggregation
//!

pub mod model;
mod column;
pub mod event;

pub use mongodb_ro_derive::*;

