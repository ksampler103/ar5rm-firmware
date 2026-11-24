//! Global simulator state. The firmware's `Api` callbacks receive no
//! context pointer, so — exactly like the firmware's own `CTX` static —
//! the harness keeps its state in a global the callbacks reach into.
//! The runner is single-threaded; a Mutex keeps that honest.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use crate::fb::Framebuffer;
use crate::fwui::list::Cache as ListCache;
use crate::rf::RfModel;

pub const FLASH_SIZE: usize = 0x20_0000; // 2 MiB mock NOR (real chip is larger; apps live below 0x195000)
pub const OVERLAY_APP_ADDR: u32 = 0x14_C000;
pub const MAX_CHANNELS: usize = 1024;
pub const SETTINGS_LEN: usize = 168;
pub const FM_CHANNEL_COUNT: usize = 30;

pub struct SimState {
    pub fb: Framebuffer,
    /// True only while an AppEvent::Draw is being dispatched — the
    /// firmware passes a null LCD pointer for every other event, which
    /// makes draw calls no-ops; the harness mirrors that with this flag.
    pub drawing: bool,
    /// Firmware clears the full screen to black before the first Draw
    /// after Enter / a segment Chain.
    pub full_clear_pending: bool,
    pub list_cache: Option<ListCache>,

    /// Virtual clock in 100us units (the firmware's uptime unit).
    /// Scenario steps advance it; `now_100us` additionally folds in
    /// real elapsed time so app busy-waits on uptime (spectrum's
    /// dwell loops) terminate like they do on hardware.
    pub clock_100us: u64,
    /// Real-time anchor for `now_100us`.
    pub real_start: std::time::Instant,
    /// UTC (seconds since 2000-01-01) as of `utc_clock_100us`.
    pub utc_base: u32,
    pub utc_clock_100us: u64,

    // --- radio config the app has programmed ---
    pub rx_freq_hz: u32,
    pub tx_freq_hz: u32,
    pub modulation: u8,
    pub power: u8,
    pub subaudio_tx: u16,
    pub subaudio_rx: u16,
    pub wide: bool,
    pub sql_level: u8,
    pub monitor: bool,
    pub ptt: bool,
    pub tx_enabled: bool,
    pub holds_rx: bool,
    pub backlight_hold: bool,
    pub speaker: bool,
    pub af_out: bool,
    pub agc_fix: i8,
    pub subaudio_scan_filter: bool,
    pub tone_hz_div10: Option<u16>,
    pub freq_scanning: bool,

    // --- master (standby) side the app can read/hand back to ---
    pub master_freq_hz: u32,
    pub master_wide: bool,
    pub master_subaudio: u16,
    pub side_cfg: SideCfg,

    // --- FM broadcast chip ---
    pub fm_tuned_khz: u32,
    pub fm_seek_failed: bool,
    pub fm_channels: [u16; FM_CHANNEL_COUNT],

    // --- storage ---
    pub flash: Vec<u8>,
    pub channels: Vec<[u8; 32]>,
    pub settings: [u8; SETTINGS_LEN],
    pub handoff: [u8; 64],
    pub handoff_len: u16,

    pub rf_gains: [u16; 4],

    pub rf: RfModel,

    // --- bookkeeping ---
    /// Notable events (loud stubs, faults, exits) for the run report.
    pub events: Vec<String>,
    /// Every API callback name invoked at least once (coverage report).
    pub called: BTreeSet<&'static str>,
    /// Set by app_fault/factory_reset instead of diverging.
    pub fatal: Option<String>,
}

#[derive(Clone, Copy)]
pub struct SideCfg {
    pub wide: bool,
    pub power_raw: u8,
    pub sub_rx: u16,
    pub sub_tx: u16,
    pub dir: u8,
    pub offset_hz: u32,
    pub ani: Option<u8>,
    pub freq_step: u8,
}

impl SimState {
    pub fn new() -> Self {
        SimState {
            fb: Framebuffer::new(),
            drawing: false,
            full_clear_pending: true,
            list_cache: None,
            clock_100us: 0,
            real_start: std::time::Instant::now(),
            utc_base: 0,
            utc_clock_100us: 0,
            rx_freq_hz: 146_520_000,
            tx_freq_hz: 146_520_000,
            modulation: 0,
            power: 0,
            subaudio_tx: 0,
            subaudio_rx: 0,
            wide: true,
            sql_level: 3,
            monitor: false,
            ptt: false,
            tx_enabled: false,
            holds_rx: false,
            backlight_hold: false,
            speaker: false,
            af_out: false,
            agc_fix: -1,
            subaudio_scan_filter: false,
            tone_hz_div10: None,
            freq_scanning: false,
            master_freq_hz: 146_520_000,
            master_wide: true,
            master_subaudio: 0,
            side_cfg: SideCfg {
                wide: true,
                power_raw: 0,
                sub_rx: 0,
                sub_tx: 0,
                dir: 0,
                offset_hz: 0,
                ani: None,
                freq_step: 0,
            },
            fm_tuned_khz: 0,
            fm_seek_failed: false,
            fm_channels: [0xFFFF; FM_CHANNEL_COUNT],
            flash: vec![0xFF; FLASH_SIZE],
            channels: vec![[0xFF; 32]; MAX_CHANNELS],
            settings: [0; SETTINGS_LEN],
            handoff: [0; 64],
            handoff_len: 0,
            rf_gains: [24, 16, 32, 40],
            rf: RfModel::default(),
            events: Vec::new(),
            called: BTreeSet::new(),
            fatal: None,
        }
    }

    /// Scenario clock + real elapsed time, in 100us units. Used for
    /// the uptime API; the RF model and event scheduling stay on the
    /// pure scenario clock for determinism.
    pub fn now_100us(&self) -> u64 {
        self.clock_100us + (self.real_start.elapsed().as_micros() / 100) as u64
    }

    pub fn utc_now(&self) -> u32 {
        let elapsed = self.clock_100us.saturating_sub(self.utc_clock_100us) / 10_000;
        self.utc_base.wrapping_add(elapsed as u32)
    }

    pub fn set_utc(&mut self, secs: u32) {
        self.utc_base = secs;
        self.utc_clock_100us = self.clock_100us;
    }

    pub fn log_event(&mut self, msg: String) {
        println!("[sim] {msg}");
        self.events.push(msg);
    }

    /// Current squelch state per the RF model: monitor forces it open;
    /// otherwise the level at the tuned frequency must clear the
    /// threshold (model base + 3 dB per sql level above/below 3).
    pub fn squelch_open(&self) -> bool {
        if self.monitor {
            return true;
        }
        let threshold = self.rf.sql_open_dbm + (self.sql_level as i32 - 3) * 3;
        self.rf.dbm_at(self.rx_freq_hz, self.clock_100us) >= threshold
    }

    /// Raw RSSI in the FD6818 scale the apps expect (dBm = raw - 170).
    pub fn rssi_raw(&self) -> u16 {
        let dbm = self.rf.dbm_at(self.rx_freq_hz, self.clock_100us);
        (dbm + 170).clamp(0, 255) as u16
    }
}

static STATE: OnceLock<Mutex<SimState>> = OnceLock::new();

pub fn init(state: SimState) {
    let _ = STATE.set(Mutex::new(state));
}

pub fn with<R>(f: impl FnOnce(&mut SimState) -> R) -> R {
    let m = STATE.get().expect("sim state not initialised");
    let mut guard = m.lock().unwrap();
    f(&mut guard)
}

/// Record an API call for the coverage report. Called at the top of
/// every callback in api.rs.
pub fn note_call(name: &'static str) {
    with(|s| {
        s.called.insert(name);
    });
}
