//! ReplicantFX: the 48-slot SP-404MKII MFX engine.
//!
//! Each clip owns one insert effect; the six macros follow the hardware
//! CTRL 1..3 / SUB 4..6 table, so every slot's labels and defaults come from
//! the firmware catalog.

mod catalog;
mod effect;

pub use catalog::{fx_catalog, fx_info, FxInfo, FX_COUNT, FX_TABLE};
pub use effect::{Effect, MAX_DELAY};
