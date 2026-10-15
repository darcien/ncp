//! `ncp`: a command-line MCP client for one user.

pub mod add_entry;
pub mod binding;
pub mod cli;
pub mod client;
pub mod config;
pub mod config_edit;
pub mod credential;
pub mod elicit;
pub mod expand;
pub mod list_cache;
pub mod oauth;
pub mod paths;
mod private_file;
pub mod project;
pub mod protocol_version;
pub mod protocol_version_file;
pub mod proxy;
pub mod redact;
pub mod server;
pub mod store;
#[cfg(test)]
mod style;
pub mod text;
