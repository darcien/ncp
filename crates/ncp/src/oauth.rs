//! OAuth for HTTP servers (MCP authorization).
//!
//! rmcp does discovery, PKCE, the `resource` parameter, the `iss` check,
//! the registration and the refresh. This module adds what rmcp does not
//! do: the redirect, the stored items, and their status.

pub mod callback;
pub mod http;
pub mod items;
pub mod scopes;
pub mod store;
