//! Game-server integration services (MatchZy).
//!
//! Design: docs/matchzy-integration.md. Phase 1: registry, enrollment/CA,
//! heartbeats, bookings. Later phases add allocation, match setup, and
//! event ingestion.

pub mod ca;
pub mod registry;
pub mod setup;

pub use ca::{AGENT_CERT_VALIDITY_DAYS, CertificateAuthority, GeneratedCa, IssuedCertificate};
pub use registry::{
    ENROLLMENT_TOKEN_TTL_HOURS, EnrollmentResult, GameServerRegistryService,
    HEARTBEAT_STALENESS_SECS, generate_enrollment_token, hash_token,
};
pub use setup::{derive_map_sides, generate_connect_password, generate_reservation_token};
