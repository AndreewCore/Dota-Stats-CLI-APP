//! Core data layer for dota-stats: OpenDota client, on-disk TTL cache,
//! config loading, and typed models. Shared by the CLI and the Tauri app.

pub mod cache;
pub mod client;
pub mod config;
pub mod display;
pub mod error;
pub mod models;

pub use client::OpenDota;
pub use config::{Config, UserProfile, UsersStore};
pub use error::{Error, Result};
