//! Built-in game plugins.

pub mod cs2;

pub use cs2::{
    validate_demo_service_url, Cs2DemoClient, Cs2DemoStats, Cs2EvidenceValidator, Cs2Plugin,
    Cs2PluginWithEvidence,
};
