//! `ncp`: a command-line MCP client for one user.

pub mod cli;
pub mod client;
pub mod config;
pub mod credential;
pub mod era;
pub mod era_file;
pub mod expand;
pub mod list_cache;
pub mod oauth;
pub mod paths;
mod private_file;
pub mod proxy;
pub mod redact;
pub mod server;
pub mod store;
