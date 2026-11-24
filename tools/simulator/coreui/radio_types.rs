//! The `crate::device::radio` types the standby renderer names.
//!
//! Copied verbatim from the fork's `src/drivers/fd6818b.rs`
//! (commit 7d2de0d), where the firmware defines them; the fork's
//! `device::radio` re-exports them. Copied into each generated
//! variant crate as `src/device/radio.rs` by gen_core.py — edit
//! it here, not in gencore/.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Low,
    Mid,
    High,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Modulation {
    Fm,
    Am,
    Usb,
    /// CW: RX demodulates as USB (beat tone), TX is a bare keyed carrier.
    Cw,
    /// CW-over-FM: RX/TX both plain FM, sidetone is what actually
    /// modulates the carrier
    Cwf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SubAudio {
    None,
    /// Analog CTCSS tone, in tenths of Hz (e.g. `1000` = 100.0Hz).
    Ctcss(u16),
    /// Digital-coded squelch. `code` is the plain-binary DCS tone value
    /// (see `DCS_TABLE`, e.g. `0o023`); `inverted` selects the "I" (inverted
    /// polarity) vs "N" (normal) variant.
    Dcs {
        code: u16,
        inverted: bool,
    },
}
