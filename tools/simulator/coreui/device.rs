//! `crate::device` shim for the core-UI simulator: only the
//! `radio` types module exists here, because that is all the
//! standby renderer names. Copied into each generated variant
//! crate as `src/device.rs` by gen_core.py.

pub mod radio;
