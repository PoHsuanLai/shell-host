//! The scroll configuration and the physics derived from it, and the time a call runs at.

use crate::scroll::config::{ScrollConfig, ScrollbarSettings};
use blitz_kit::scroll::time::Elapsed;
use blitz_kit::scroll::tuning::{Env as KitEnv, Tuning as KitTuning};

/// The engine's tuning (the kit's) and the thumb's settings, copied into every surface.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Tuning {
    kit: KitTuning,
    scrollbar: ScrollbarSettings,
}

impl Tuning {
    /// The tuning `config` gives.
    pub(crate) fn new(config: ScrollConfig) -> Tuning {
        Tuning {
            kit: KitTuning::new(config.scroll),
            scrollbar: config.scrollbar,
        }
    }

    /// The scroll settings the engine's tuning was made from.
    pub(crate) fn settings(&self) -> &blitz_kit::scroll::config::ScrollSettings {
        &self.kit.settings
    }

    /// This tuning at `now`.
    pub(crate) fn at(&self, now: Elapsed) -> Env<'_> {
        Env {
            kit: self.kit.at(now),
            scrollbar: &self.scrollbar,
            now,
        }
    }
}

/// The time and the tuning a call works with: `kit` is the engine's own call context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Env<'a> {
    pub kit: KitEnv<'a>,
    pub scrollbar: &'a ScrollbarSettings,
    pub now: Elapsed,
}
