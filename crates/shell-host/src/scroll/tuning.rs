//! The scroll configuration and the physics derived from it, and the time a call runs at.

use crate::scroll::config::ScrollConfig;
use crate::scroll::engine::Physics;
use crate::tick::HostInstant;

/// The configuration and the physics derived from it, copied into every surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tuning {
    pub config: ScrollConfig,
    pub physics: Physics,
}

impl Tuning {
    /// The tuning `config` gives.
    pub(crate) fn new(config: ScrollConfig) -> Tuning {
        Tuning {
            physics: Physics::from_config(&config),
            config,
        }
    }

    /// This tuning at `now`.
    pub(crate) fn at(&self, now: HostInstant) -> Env<'_> {
        Env {
            config: &self.config,
            physics: &self.physics,
            now,
        }
    }
}

impl Default for Tuning {
    fn default() -> Tuning {
        Tuning::new(ScrollConfig::default())
    }
}

/// The time and the tuning a call works with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Env<'a> {
    pub config: &'a ScrollConfig,
    pub physics: &'a Physics,
    pub now: HostInstant,
}
