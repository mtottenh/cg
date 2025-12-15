//! Built-in game plugins.

pub mod cs2;

pub use cs2::{
    Cs2DemoClient, Cs2DemoStats, Cs2EvidenceValidator, Cs2Plugin, Cs2PluginWithEvidence,
};
