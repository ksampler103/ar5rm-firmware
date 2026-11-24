//! Mock `crate::app` for the core-UI simulator.
//!
//! The fork's standby renderer (`src/ui/standby.rs`) draws from an
//! `app::App` through a narrow set of getters. This module provides
//! an `App` with exactly those getters, backed by plain fields the
//! runner fills from Preston's real channels/settings and the RF
//! model. The observable semantics mirror `src/app/mod.rs` in the
//! fork (commit 7d2de0d): the S-meter curves, the RSSI conversion
//! (`raw - 170`) and the battery formula are copied verbatim; the
//! state itself (which channel sits on which side, what the squelch
//! is doing) is scenario state, set by the runner.
//!
//! Copied into each generated variant crate as `src/app.rs` by
//! gen_core.py — edit it here, not in gencore/.

use crate::device::radio::{Modulation, Power, SubAudio};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelDisplayMode {
    Frequency,
    Name,
    NameFreq,
}

/// The renderer reads exactly one settings field (`sql_level`, in
/// the Data Dense A-box header). The rest of the record reaches the
/// mock through the dedicated fields below, parsed by the runner
/// with the firmware's own offsets (`flash_map::Settings::from_bytes`).
pub struct Settings {
    pub sql_level: u8,
}

/// One standby side (one VFO/channel slot), mirroring the fork's
/// `app::side::Side` after `load_channel`: `freq_hz` is the RX
/// frequency, `tx_freq_hz` the TX frequency, and `freq_dir` /
/// `offset_hz` are the `shift_dir_offset()` pair the renderer
/// prints (derived from TX vs RX in channel mode).
#[derive(Clone)]
pub struct SideState {
    pub freq_hz: u32,
    pub tx_freq_hz: u32,
    pub is_channel_mode: bool,
    pub channel_num: u16,
    pub name: String,
    pub modulation: Modulation,
    pub power: Power,
    pub subaudio_tx: SubAudio,
    pub subaudio_rx: SubAudio,
    pub wide_band: bool,
    pub reversed: bool,
    pub freq_dir: u8,
    pub offset_hz: u32,
}

impl SideState {
    pub fn vfo(freq_hz: u32) -> Self {
        SideState {
            freq_hz,
            tx_freq_hz: freq_hz,
            is_channel_mode: false,
            channel_num: 0,
            name: String::new(),
            modulation: Modulation::Fm,
            power: Power::High,
            subaudio_tx: SubAudio::None,
            subaudio_rx: SubAudio::None,
            wide_band: true,
            reversed: false,
            freq_dir: 0,
            offset_hz: 0,
        }
    }
}

pub struct App {
    pub sides: [SideState; 2],
    pub master: usize,
    pub watching: usize,
    pub transmitting: bool,
    pub tx_prohibited: bool,
    pub key_locked: bool,
    pub power_save: bool,
    pub dual_standby: bool,
    pub vox_switch: bool,
    pub settings: Settings,
    pub channel_display_mode: ChannelDisplayMode,
    /// UTC seconds since 2000-01-01; `bf5rh_abi::UTC_NOT_SET` (0)
    /// means unset, exactly as in the firmware.
    pub utc_secs: u32,
    pub tx_elapsed_seconds: u32,
    /// Watching side's RSSI in the FD6818 scale (`rssi_dbm` = raw - 170).
    pub rssi_raw: u8,
    /// Squelch/audio state of the watching side.
    pub squelch_open: bool,
    pub monitor: bool,
    pub blink_on: bool,
    /// (side, peak dBm) of the last signal, as the firmware's
    /// `last_signal_peak` reports it.
    pub last_signal: Option<(usize, i32)>,
    pub mic_level: u8,
    pub battery_bars: u8,
    pub battery_cv: u16,
    pub no_channels_notice: bool,
    // Input states. The rendered scenarios are idle, so these stay
    // empty/inactive; they exist so every renderer path compiles
    // and behaves as the firmware's idle state does.
    pub dtmf_active: bool,
    pub dtmf_digits: Vec<u8>,
}

impl App {
    // --- snapshot getters (Snapshot::capture in standby.rs) ---
    pub fn is_transmitting(&self) -> bool {
        self.transmitting
    }
    pub fn master_index(&self) -> usize {
        self.master
    }
    pub fn watching_index(&self) -> usize {
        self.watching
    }
    pub fn side_freq_hz(&self, index: usize) -> u32 {
        self.sides[index].freq_hz
    }
    pub fn side_tx_freq_hz(&self, index: usize) -> u32 {
        self.sides[index].tx_freq_hz
    }
    pub fn side_is_channel_mode(&self, index: usize) -> bool {
        self.sides[index].is_channel_mode
    }
    pub fn side_channel_num(&self, index: usize) -> u16 {
        self.sides[index].channel_num
    }
    pub fn side_name_str(&self, index: usize) -> &str {
        &self.sides[index].name
    }
    pub fn side_modulation(&self, index: usize) -> Modulation {
        self.sides[index].modulation
    }
    pub fn side_power(&self, index: usize) -> Power {
        self.sides[index].power
    }
    pub fn side_subaudio_tx(&self, index: usize) -> SubAudio {
        self.sides[index].subaudio_tx
    }
    pub fn side_subaudio_rx(&self, index: usize) -> SubAudio {
        self.sides[index].subaudio_rx
    }
    pub fn side_offset_hz(&self, index: usize) -> u32 {
        self.sides[index].offset_hz
    }
    pub fn side_freq_dir(&self, index: usize) -> u8 {
        self.sides[index].freq_dir
    }
    pub fn side_wide_band(&self, index: usize) -> bool {
        self.sides[index].wide_band
    }
    pub fn side_reversed(&self, index: usize) -> bool {
        self.sides[index].reversed
    }
    pub fn power_save_active(&self) -> bool {
        self.power_save
    }
    pub fn dual_standby_enabled(&self) -> bool {
        self.dual_standby
    }
    pub fn vox_enabled(&self) -> bool {
        self.vox_switch
    }
    pub fn is_key_locked(&self) -> bool {
        self.key_locked
    }
    pub fn tx_prohibited(&self) -> bool {
        self.tx_prohibited
    }
    pub fn no_channels_notice(&self) -> bool {
        self.no_channels_notice
    }

    // --- input states (idle in the rendered scenarios) ---
    pub fn dtmf_dial_active(&self) -> bool {
        self.dtmf_active
    }
    pub fn dtmf_dial_len(&self) -> usize {
        self.dtmf_digits.len()
    }
    pub fn dtmf_dial_digit(&self, idx: usize) -> u8 {
        self.dtmf_digits.get(idx).copied().unwrap_or(0)
    }
    pub fn dtmf_dial_capacity(&self) -> usize {
        16 // DTMF_DIAL_MAX_DIGITS in the firmware
    }
    pub fn channel_input_len(&self) -> usize {
        0
    }
    pub fn freq_input_len(&self) -> usize {
        0
    }
    pub fn freq_input_digit(&self, _idx: usize) -> u8 {
        0
    }

    // --- status / dynamics ---
    pub fn utc_secs(&self) -> u32 {
        self.utc_secs
    }
    pub fn tx_elapsed_seconds(&self) -> u32 {
        self.tx_elapsed_seconds
    }
    pub fn settings(&self) -> &Settings {
        &self.settings
    }
    pub fn channel_display_mode(&self) -> ChannelDisplayMode {
        self.channel_display_mode
    }
    pub fn battery_bars(&self) -> u8 {
        self.battery_bars
    }
    pub fn battery_voltage_cv(&self) -> u16 {
        self.battery_cv
    }
    pub fn rssi_dbm(&self) -> i32 {
        rssi_raw_to_dbm(self.rssi_raw as u16)
    }
    pub fn rssi_open(&self) -> bool {
        self.squelch_open
    }
    pub fn audio_open(&self) -> bool {
        self.squelch_open
    }
    pub fn is_monitor(&self) -> bool {
        self.monitor
    }
    pub fn rx_blink_on(&self) -> bool {
        self.blink_on
    }
    pub fn last_signal_peak(&self) -> Option<(usize, i32)> {
        self.last_signal
    }
    pub fn last_signal_side(&self) -> Option<usize> {
        self.last_signal.map(|(side, _)| side)
    }
    pub fn s_meter_level(&self) -> u8 {
        s_meter_level_of(self.rssi_dbm())
    }
    pub fn s_meter_s_number(&self) -> u8 {
        s_meter_s_number_of(self.rssi_dbm())
    }
    pub fn s_meter_over_s9_dbm(&self) -> i32 {
        s_meter_over_s9_of(self.rssi_dbm())
    }
    pub fn mic_level(&self) -> u8 {
        self.mic_level
    }
}

// --- copied verbatim from the fork's src/app/mod.rs (7d2de0d) ---

/// TODO: calibrate per band
const RSSI_DBM_BASE: i16 = 160 + 10;

pub fn rssi_raw_to_dbm(raw: u16) -> i32 {
    raw as i32 - RSSI_DBM_BASE as i32
}

fn map(x: i32, in_min: i32, in_max: i32, out_min: i32, out_max: i32) -> i32 {
    (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min
}

pub fn s_meter_level_of(dbm: i32) -> u8 {
    let pos = (-dbm).clamp(53, 141);
    if pos >= 93 {
        map(pos, 141, 93, 1, 9).clamp(1, 9) as u8
    } else {
        (9 + map(pos, 93, 53, 0, 4).clamp(0, 4)) as u8
    }
}

pub fn s_meter_s_number_of(dbm: i32) -> u8 {
    map((-dbm).clamp(53, 141), 141, 93, 1, 9).clamp(1, 9) as u8
}

pub fn s_meter_over_s9_of(dbm: i32) -> i32 {
    (93 - (-dbm).clamp(53, 141)).max(0)
}
