//! Game-server integration handlers (MatchZy).
//!
//! `admin` — the registry CRUD/enrollment surface (OpenAPI-documented,
//! `admin.servers.manage`). `agent` — the server-facing enrollment endpoint
//! and the agent WebSocket channel (excluded from the public OpenAPI spec,
//! like `handlers/internal.rs`). Design: docs/matchzy-integration.md §5–§6.
//! `updates` — LinuxGSM updates and the portal-managed MatchZy overrides
//! (docs/gameserver-lgsm-updates-design.md).

pub mod admin;
pub mod agent;
pub mod console;
pub mod match_server;
pub mod matchzy;
pub mod substitutions;
pub mod updates;
