//! The mock firmware: an implementation of `bf5rh_abi::Api` whose
//! semantics follow the firmware's own implementations in
//! `src/app/overlay.rs` as closely as a host mock can.
//!
//! Honesty rules (per the project brief):
//! - Every callback records its name for the coverage report.
//! - Callbacks whose behaviour is a *model* rather than a reproduction
//!   (RF levels, FM seek, battery, RF gains...) log a one-time
//!   `[stub]` note saying so.
//! - `app_fault` / `factory_reset` diverge on the radio (MCU reset);
//!   here they abort the run with a distinct exit code and a loud log.

use std::collections::HashSet;
use std::sync::Mutex;

use bf5rh_abi::{Api, ListRow};
use embedded_graphics::mono_font::ascii::{FONT_5X8, FONT_6X10, FONT_9X18};
use embedded_graphics::mono_font::{MonoFont, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;

use crate::fwui::list::{draw_list as draw_list_widget, Cache as ListCache, ListSource};
use crate::state::{self, SimState, FLASH_SIZE, OVERLAY_APP_ADDR, SETTINGS_LEN};
use crate::subaudio::{self, SubAudio};

fn note(name: &'static str) {
    state::note_call(name);
}

thread_local! {
    static STUB_NOTED: std::cell::RefCell<HashSet<&'static str>> =
        std::cell::RefCell::new(HashSet::new());
}

/// One-time loud note that `name` is a modelled simplification.
fn stub(name: &'static str, detail: &str) {
    let first = STUB_NOTED.with(|s| s.borrow_mut().insert(name));
    if first {
        state::with(|s| s.log_event(format!("[stub] {name}: {detail}")));
    }
}

// ---------------------------------------------------------------- time

extern "C" fn api_uptime_100us() -> u32 {
    note("uptime_100us");
    state::with(|s| s.now_100us() as u32)
}

extern "C" fn api_delay_ms(ms: u32) {
    note("delay_ms");
    state::with(|s| s.clock_100us += ms as u64 * 10);
}

// ---------------------------------------------------------------- drawing
// All draw calls are no-ops unless a Draw event is being dispatched,
// mirroring the firmware's null-LCD behaviour for other events.

extern "C" fn api_fill_rect(x: i16, y: i16, w: u16, h: u16, rgb565: u16) {
    note("fill_rect");
    state::with(|s| {
        if !s.drawing {
            return;
        }
        let color = Rgb565::from(RawU16::new(rgb565));
        Rectangle::new(
            Point::new(x as i32, y as i32),
            Size::new(w as u32, h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(&mut s.fb)
        .ok();
    });
}

extern "C" fn api_blit(x: i16, y: i16, w: u16, h: u16, px: *const u16) {
    note("blit");
    if px.is_null() {
        return;
    }
    state::with(|s| {
        if !s.drawing {
            return;
        }
        let n = w as usize * h as usize;
        let pixels = unsafe { std::slice::from_raw_parts(px, n) };
        let area = Rectangle::new(
            Point::new(x as i32, y as i32),
            Size::new(w as u32, h as u32),
        );
        s.fb.fill_contiguous(
            &area,
            pixels.iter().map(|&raw| Rgb565::from(RawU16::new(raw))),
        )
        .ok();
    });
}

extern "C" fn api_draw_line(x0: i16, y0: i16, x1: i16, y1: i16, rgb565: u16) {
    note("draw_line");
    state::with(|s| {
        if !s.drawing {
            return;
        }
        let color = Rgb565::from(RawU16::new(rgb565));
        Line::new(Point::new(x0 as i32, y0 as i32), Point::new(x1 as i32, y1 as i32))
            .into_styled(PrimitiveStyle::with_stroke(color, 1))
            .draw(&mut s.fb)
            .ok();
    });
}

extern "C" fn api_draw_circle(x: i16, y: i16, r: u16, rgb565: u16, filled: bool) {
    note("draw_circle");
    state::with(|s| {
        if !s.drawing {
            return;
        }
        let color = Rgb565::from(RawU16::new(rgb565));
        let style = if filled {
            PrimitiveStyle::with_fill(color)
        } else {
            PrimitiveStyle::with_stroke(color, 1)
        };
        let d = 2 * r as u32;
        Circle::new(Point::new(x as i32 - r as i32, y as i32 - r as i32), d)
            .into_styled(style)
            .draw(&mut s.fb)
            .ok();
    });
}

fn draw_text_with_font(
    s: &mut SimState,
    text: &str,
    x: i16,
    y: i16,
    font: &MonoFont<'_>,
    fg: Rgb565,
    bg: Rgb565,
) -> u16 {
    let style = MonoTextStyleBuilder::new()
        .font(font)
        .text_color(fg)
        .background_color(bg)
        .build();
    let baseline_y = y as i32 + font.baseline as i32;
    Text::new(text, Point::new(x as i32, baseline_y), style)
        .draw(&mut s.fb)
        .ok();
    (x as i32 + font.character_size.width as i32 * text.chars().count() as i32) as u16
}

extern "C" fn api_draw_text(
    x: i16,
    y: i16,
    text: *const u8,
    len: u16,
    fg: u16,
    bg: u16,
    font: u8,
) -> u16 {
    note("draw_text");
    if text.is_null() {
        return x as u16;
    }
    state::with(|s| {
        if !s.drawing {
            return x as u16;
        }
        let bytes = unsafe { std::slice::from_raw_parts(text, len as usize) };
        let text = core::str::from_utf8(bytes).unwrap_or("");
        let fg = Rgb565::from(RawU16::new(fg));
        let bg = Rgb565::from(RawU16::new(bg));
        match font {
            2 => draw_text_with_font(s, text, x, y, &FONT_9X18, fg, bg),
            1 => draw_text_with_font(s, text, x, y, &FONT_6X10, fg, bg),
            _ => draw_text_with_font(s, text, x, y, &FONT_5X8, fg, bg),
        }
    })
}

// ---------------------------------------------------------------- RF config

extern "C" fn api_set_rx_freq(hz: u32) {
    note("set_rx_freq");
    state::with(|s| s.rx_freq_hz = hz);
}

extern "C" fn api_set_tx_freq(hz: u32) {
    note("set_tx_freq");
    state::with(|s| s.tx_freq_hz = hz);
}

extern "C" fn api_set_ptt(on: bool) {
    note("set_ptt");
    state::with(|s| {
        if on && !s.tx_enabled {
            s.log_event("set_ptt(true) refused: app never called set_tx_enabled(true) (matches firmware gating)".into());
        }
        s.ptt = on;
    });
}

extern "C" fn api_read_rssi() -> u16 {
    note("read_rssi");
    stub("read_rssi", "level comes from the sim RF model, not a receiver");
    state::with(|s| s.rssi_raw())
}

extern "C" fn api_set_modulation(mode: u8) {
    note("set_modulation");
    state::with(|s| s.modulation = mode);
}

extern "C" fn api_set_power(level: u8) {
    note("set_power");
    state::with(|s| s.power = level);
}

extern "C" fn api_set_subaudio_tx(code: u16) {
    note("set_subaudio_tx");
    state::with(|s| s.subaudio_tx = code);
}

extern "C" fn api_set_subaudio_rx(code: u16) {
    note("set_subaudio_rx");
    state::with(|s| s.subaudio_rx = code);
}

extern "C" fn api_set_bandwidth(wide: bool) {
    note("set_bandwidth");
    state::with(|s| s.wide = wide);
}

extern "C" fn api_set_sql_level(level: u8) {
    note("set_sql_level");
    state::with(|s| s.sql_level = level);
}

extern "C" fn api_set_monitor(on: bool) {
    note("set_monitor");
    state::with(|s| s.monitor = on);
}

extern "C" fn api_enter_rx() {
    note("enter_rx");
    state::with(|s| {
        s.holds_rx = true;
    });
}

extern "C" fn api_retune_rx(hz: u32, wide: bool) {
    note("retune_rx");
    state::with(|s| {
        s.rx_freq_hz = hz;
        s.wide = wide;
    });
}

extern "C" fn api_squelch_open() -> bool {
    note("squelch_open");
    stub(
        "squelch_open",
        "threshold is the RF model's sql_open_dbm + 3 dB per sql level from 3",
    );
    state::with(|s| s.squelch_open())
}

extern "C" fn api_tx_state() -> u32 {
    note("tx_state");
    state::with(|s| {
        let transmitting = s.ptt && s.tx_enabled;
        let refused = s.ptt && !s.tx_enabled;
        transmitting as u32 | (refused as u32) << 1
    })
}

extern "C" fn api_set_tx_enabled(on: bool) {
    note("set_tx_enabled");
    state::with(|s| s.tx_enabled = on);
}

extern "C" fn api_set_holds_rx(on: bool) {
    note("set_holds_rx");
    state::with(|s| s.holds_rx = on);
}

extern "C" fn api_set_backlight_hold(on: bool) {
    note("set_backlight_hold");
    state::with(|s| s.backlight_hold = on);
}

extern "C" fn api_set_af_out(on: bool) {
    note("set_af_out");
    state::with(|s| s.af_out = on);
}

extern "C" fn api_set_agc_fix(rank: i8) {
    note("set_agc_fix");
    state::with(|s| s.agc_fix = rank);
}

extern "C" fn api_read_rf_gains(out: *mut u16) -> bool {
    note("read_rf_gains");
    stub("read_rf_gains", "fixed plausible gain values, adjustable via adjust_rf_gain");
    if out.is_null() {
        return false;
    }
    state::with(|s| unsafe {
        for i in 0..4 {
            out.add(i).write(s.rf_gains[i]);
        }
    });
    true
}

extern "C" fn api_adjust_rf_gain(menu: u8, up: bool) {
    note("adjust_rf_gain");
    if menu > 3 {
        return;
    }
    state::with(|s| {
        let g = &mut s.rf_gains[menu as usize];
        *g = if up {
            (*g + 1).min(63)
        } else {
            g.saturating_sub(1)
        };
    });
}

// ---------------------------------------------------------------- audio

extern "C" fn api_tone_on(hz_div10: u16) {
    note("tone_on");
    state::with(|s| s.tone_hz_div10 = Some(hz_div10));
}

extern "C" fn api_tone_off() {
    note("tone_off");
    state::with(|s| s.tone_hz_div10 = None);
}

extern "C" fn api_set_speaker(on: bool) {
    note("set_speaker");
    state::with(|s| s.speaker = on);
}

extern "C" fn api_read_mic_level() -> u8 {
    note("read_mic_level");
    stub("read_mic_level", "always 0 (silent room)");
    0
}

// ---------------------------------------------------------------- FM radio

extern "C" fn api_fm_tune_khz(freq_khz: u32) {
    note("fm_tune_khz");
    state::with(|s| {
        s.fm_tuned_khz = freq_khz;
        s.fm_seek_failed = false;
    });
}

extern "C" fn api_fm_seek(up: bool) {
    note("fm_seek");
    stub("fm_seek", "instant seek to the next modelled station");
    state::with(|s| {
        let cur = s.fm_tuned_khz;
        let mut stations: Vec<u32> = s.rf.fm_stations.iter().map(|&(k, _)| k).collect();
        stations.sort_unstable();
        let next = if up {
            stations
                .iter()
                .copied()
                .find(|&k| k > cur)
                .or_else(|| stations.first().copied())
        } else {
            stations
                .iter()
                .copied()
                .rev()
                .find(|&k| k < cur)
                .or_else(|| stations.last().copied())
        };
        match next {
            Some(k) => {
                s.fm_tuned_khz = k;
                s.fm_seek_failed = false;
            }
            None => {
                s.fm_seek_failed = true;
            }
        }
    });
}

extern "C" fn api_fm_status() -> u32 {
    note("fm_status");
    state::with(|s| {
        let is_station = s.rf.fm_is_station(s.fm_tuned_khz);
        let rssi = s.rf.fm_rssi_at(s.fm_tuned_khz) as u32;
        1u32 | ((s.fm_seek_failed as u32) << 1) | ((is_station as u32) << 2) | (rssi << 8)
    })
}

extern "C" fn api_fm_tuned_freq_khz() -> u32 {
    note("fm_tuned_freq_khz");
    state::with(|s| s.fm_tuned_khz)
}

extern "C" fn api_fm_power_off() {
    note("fm_power_off");
    state::with(|s| {
        s.fm_tuned_khz = 0;
        s.fm_seek_failed = false;
    });
}

extern "C" fn api_fm_channels_load(buf: *mut u8, len: u16) -> bool {
    note("fm_channels_load");
    const N: usize = crate::state::FM_CHANNEL_COUNT * 2;
    if buf.is_null() || (len as usize) < N {
        return false;
    }
    state::with(|s| {
        let slice = unsafe { std::slice::from_raw_parts_mut(buf, N) };
        for (pair, v) in slice.chunks_exact_mut(2).zip(s.fm_channels.iter()) {
            pair.copy_from_slice(&v.to_le_bytes());
        }
    });
    true
}

extern "C" fn api_fm_channels_save(buf: *const u8, len: u16) -> bool {
    note("fm_channels_save");
    const N: usize = crate::state::FM_CHANNEL_COUNT * 2;
    if buf.is_null() || (len as usize) < N {
        return false;
    }
    state::with(|s| {
        let slice = unsafe { std::slice::from_raw_parts(buf, N) };
        for (slot, pair) in s.fm_channels.iter_mut().zip(slice.chunks_exact(2)) {
            *slot = u16::from_le_bytes([pair[0], pair[1]]);
        }
    });
    true
}

// ---------------------------------------------------------------- NOR flash

extern "C" fn api_nor_read(addr: u32, buf: *mut u8, len: u32) -> bool {
    note("nor_read");
    if buf.is_null() {
        return false;
    }
    state::with(|s| {
        let addr = addr as usize;
        if addr >= FLASH_SIZE {
            return false;
        }
        let n = (len as usize).min(FLASH_SIZE - addr);
        let slice = unsafe { std::slice::from_raw_parts_mut(buf, n) };
        slice.copy_from_slice(&s.flash[addr..addr + n]);
        true
    })
}

extern "C" fn api_nor_write(addr: u32, buf: *const u8, len: u32) -> bool {
    note("nor_write");
    if buf.is_null() || addr < OVERLAY_APP_ADDR {
        return false; // firmware fence: writes below the app region refused
    }
    state::with(|s| {
        let addr = addr as usize;
        if addr >= FLASH_SIZE {
            return false;
        }
        let n = (len as usize).min(FLASH_SIZE - addr);
        let slice = unsafe { std::slice::from_raw_parts(buf, n) };
        // NOR semantics: a write can only clear bits (1 -> 0).
        for i in 0..n {
            s.flash[addr + i] &= slice[i];
        }
        true
    })
}

extern "C" fn api_nor_erase_sector(addr: u32) -> bool {
    note("nor_erase_sector");
    if addr < OVERLAY_APP_ADDR {
        return false; // firmware fence
    }
    state::with(|s| {
        let base = (addr as usize) & !0xFFF;
        if base >= FLASH_SIZE {
            return false;
        }
        let end = (base + 0x1000).min(FLASH_SIZE);
        s.flash[base..end].fill(0xFF);
        true
    })
}

// ---------------------------------------------------------------- formatting

extern "C" fn api_fmt_u32(v: u32, out: *mut u8) -> u16 {
    note("fmt_u32");
    if out.is_null() {
        return 0;
    }
    let text = format!("{v}");
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len()) };
    text.len() as u16
}

extern "C" fn api_fmt_freq(hz: u32, out: *mut u8) -> u16 {
    note("fmt_freq");
    if out.is_null() {
        return 0;
    }
    let mhz = hz / 1_000_000;
    let frac = (hz % 1_000_000) / 10;
    let text = format!("{mhz}.{frac:05}");
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len()) };
    text.len() as u16
}

// ---------------------------------------------------------------- channels

extern "C" fn api_chan_read(num: u16, out: *mut u8, out_len: u16) -> bool {
    note("chan_read");
    if out.is_null() || out_len < 32 {
        return false;
    }
    state::with(|s| {
        let Some(rec) = s.channels.get(num as usize) else {
            return false;
        };
        if rec.iter().all(|&b| b == 0xFF) {
            return false; // empty, like the firmware's is_channel_empty
        }
        unsafe { std::ptr::copy_nonoverlapping(rec.as_ptr(), out, 32) };
        true
    })
}

extern "C" fn api_chan_write(num: u16, buf: *const u8, len: u16) -> bool {
    note("chan_write");
    if buf.is_null() || len < 32 || num as usize >= crate::state::MAX_CHANNELS {
        return false;
    }
    state::with(|s| {
        let slice = unsafe { std::slice::from_raw_parts(buf, 32) };
        s.channels[num as usize].copy_from_slice(slice);
    });
    true
}

// ---------------------------------------------------------------- settings

fn coord_or_unset(v: i32, limit: i32) -> i32 {
    if v >= -limit && v <= limit {
        v
    } else {
        i32::MAX // SETTING_COORD_NOT_SET
    }
}

extern "C" fn api_settings_get(id: u16) -> u32 {
    note("settings_get");
    state::with(|s| {
        let b = &s.settings;
        match id {
            0 => b[0] as u32,              // sql_level
            1 => b[8] as u32,              // tot_level
            2 => (b[6] != 0) as u32,       // vox_switch
            3 => b[7].clamp(1, 9) as u32,  // vox_level
            4 => (b[9] != 0) as u32,       // beeps_switch
            5 => coord_or_unset(
                i32::from_le_bytes([b[32], b[33], b[34], b[35]]),
                9_000_000,
            ) as u32, // obs_lat
            6 => coord_or_unset(
                i32::from_le_bytes([b[36], b[37], b[38], b[39]]),
                18_000_000,
            ) as u32, // obs_lon
            7 => {
                stub("settings_get(7)", "chip id modelled as 0");
                0
            }
            _ => 0,
        }
    })
}

extern "C" fn api_settings_read(buf: *mut u8, len: u16) -> bool {
    note("settings_read");
    if buf.is_null() || (len as usize) < SETTINGS_LEN {
        return false;
    }
    state::with(|s| unsafe {
        std::ptr::copy_nonoverlapping(s.settings.as_ptr(), buf, SETTINGS_LEN);
    });
    true
}

extern "C" fn api_settings_preview(buf: *const u8, len: u16) -> bool {
    note("settings_preview");
    if buf.is_null() || (len as usize) < SETTINGS_LEN {
        return false;
    }
    state::with(|s| {
        let slice = unsafe { std::slice::from_raw_parts(buf, SETTINGS_LEN) };
        s.settings.copy_from_slice(slice);
        s.sql_level = s.settings[0];
    });
    true
}

extern "C" fn api_settings_commit(buf: *const u8, len: u16) -> bool {
    note("settings_commit");
    if !api_settings_preview(buf, len) {
        return false;
    }
    state::with(|s| s.log_event("settings_commit: record stored in mock settings".into()));
    true
}

extern "C" fn api_side_cfg_read(buf: *mut u8, len: u16) -> bool {
    note("side_cfg_read");
    if buf.is_null() || len < 14 {
        return false;
    }
    state::with(|s| {
        let c = &s.side_cfg;
        let mut out = [0u8; 14];
        out[0] = c.wide as u8;
        out[1] = c.power_raw;
        out[2..4].copy_from_slice(&c.sub_rx.to_le_bytes());
        out[4..6].copy_from_slice(&c.sub_tx.to_le_bytes());
        out[6] = c.dir;
        out[7..11].copy_from_slice(&c.offset_hz.to_le_bytes());
        let ani: i16 = c.ani.map_or(-1, |t| t as i16);
        out[11..13].copy_from_slice(&ani.to_le_bytes());
        out[13] = c.freq_step;
        unsafe { std::ptr::copy_nonoverlapping(out.as_ptr(), buf, 14) };
    });
    true
}

extern "C" fn api_side_cfg_write(buf: *const u8, len: u16) -> bool {
    note("side_cfg_write");
    if buf.is_null() || len < 14 {
        return false;
    }
    state::with(|s| {
        let raw = unsafe { std::slice::from_raw_parts(buf, 14) };
        let ani_raw = i16::from_le_bytes([raw[11], raw[12]]);
        s.side_cfg = crate::state::SideCfg {
            wide: raw[0] != 0,
            power_raw: raw[1],
            sub_rx: u16::from_le_bytes([raw[2], raw[3]]),
            sub_tx: u16::from_le_bytes([raw[4], raw[5]]),
            dir: raw[6],
            offset_hz: u32::from_le_bytes([raw[7], raw[8], raw[9], raw[10]]),
            ani: (ani_raw >= 0).then_some(ani_raw as u8),
            freq_step: raw[13],
        };
        s.master_wide = s.side_cfg.wide;
        s.log_event("side_cfg_write: master side config updated in mock".into());
    });
    true
}

extern "C" fn api_battery_raw12_avg() -> u16 {
    note("battery_raw12_avg");
    stub("battery_raw12_avg", "fixed raw ADC value 2500");
    2500
}

extern "C" fn api_factory_reset() -> ! {
    note("factory_reset");
    eprintln!("[sim] FATAL: app called factory_reset — on the radio this wipes settings and resets the MCU. Aborting the run.");
    std::process::exit(71);
}

// ---------------------------------------------------------------- master side

extern "C" fn api_get_master_freq() -> u32 {
    note("get_master_freq");
    state::with(|s| s.master_freq_hz)
}

extern "C" fn api_get_master_wide() -> bool {
    note("get_master_wide");
    state::with(|s| s.master_wide)
}

extern "C" fn api_save_master_vfo(freq_hz: u32, subaudio_code: u16) {
    note("save_master_vfo");
    state::with(|s| {
        s.master_freq_hz = freq_hz;
        s.master_subaudio = subaudio_code;
        s.log_event(format!(
            "save_master_vfo: master VFO handed {freq_hz} Hz, subaudio code {subaudio_code}"
        ));
    });
}

extern "C" fn api_save_master_subaudio(code: u16, _also_tx: bool) {
    note("save_master_subaudio");
    state::with(|s| s.master_subaudio = code);
}

// ---------------------------------------------------------------- list UI

struct BufferListSource<'a> {
    rows: &'a [ListRow],
    window_start: usize,
    total: usize,
}

impl<'a> BufferListSource<'a> {
    fn window_row(&self, index: usize) -> Option<&'a ListRow> {
        if index < self.window_start {
            return None;
        }
        self.rows.get(index - self.window_start)
    }
}

impl<'a> ListSource for BufferListSource<'a> {
    fn row_count(&mut self) -> usize {
        self.total
    }

    fn label(&mut self, index: usize, w: &mut dyn core::fmt::Write) {
        if let Some(row) = self.window_row(index) {
            let end = row
                .label
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(row.label.len());
            if let Ok(text) = core::str::from_utf8(&row.label[..end]) {
                let _ = w.write_str(text);
            }
        }
    }

    fn value(&mut self, index: usize, w: &mut dyn core::fmt::Write) -> bool {
        if let Some(row) = self.window_row(index) {
            if row.has_value {
                let end = row
                    .value
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(row.value.len());
                if let Ok(text) = core::str::from_utf8(&row.value[..end]) {
                    let _ = w.write_str(text);
                }
                return true;
            }
        }
        false
    }

    fn cursor(&mut self, index: usize) -> Option<usize> {
        match self.window_row(index) {
            Some(row) if row.cursor >= 0 => Some(row.cursor as usize),
            _ => None,
        }
    }
}

extern "C" fn api_draw_list(
    title: *const u8,
    title_len: u16,
    rows: *const ListRow,
    row_count: u16,
    window_start: u16,
    selected: u16,
    total: u16,
    show_arrows: bool,
) {
    note("draw_list");
    state::with(|s| {
        if !s.drawing {
            return;
        }
        let title_str = if title.is_null() {
            ""
        } else {
            let bytes = unsafe { std::slice::from_raw_parts(title, title_len as usize) };
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            core::str::from_utf8(&bytes[..end]).unwrap_or("")
        };
        let rows_slice: &[ListRow] = if rows.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(rows, row_count as usize) }
        };
        let mut source = BufferListSource {
            rows: rows_slice,
            window_start: window_start as usize,
            total: total as usize,
        };
        if s.list_cache.is_none() {
            s.list_cache = Some(ListCache::new());
        }
        // Split borrows: the widget draws into the framebuffer while the
        // cache lives in the same state struct.
        let mut fb = std::mem::replace(&mut s.fb, crate::fb::Framebuffer::new());
        let cache = s.list_cache.as_mut().unwrap();
        draw_list_widget(&mut fb, title_str, &mut source, selected as usize, show_arrows, cache);
        s.fb = fb;
    });
}

// ---------------------------------------------------------------- subaudio

extern "C" fn api_subaudio_index_of_code(code: u16) -> i32 {
    note("subaudio_index_of_code");
    subaudio::index_of(subaudio::from_code(code))
}

extern "C" fn api_subaudio_code_of_index(v: i32) -> u16 {
    note("subaudio_code_of_index");
    subaudio::to_code(subaudio::from_index(v))
}

extern "C" fn api_subaudio_format(v: i32, out: *mut u8, cap: u16) -> u16 {
    note("subaudio_format");
    if out.is_null() || cap == 0 {
        return 0;
    }
    let text = match subaudio::from_index(v) {
        SubAudio::None => "OFF".to_string(),
        SubAudio::Ctcss(hz) => format!("{}.{}Hz", hz / 10, hz % 10),
        SubAudio::Dcs { code, inverted } => {
            format!("D{:03o}{}", code, if inverted { "I" } else { "N" })
        }
    };
    let n = text.len().min(cap as usize);
    unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), out, n) };
    n as u16
}

extern "C" fn api_detect_subaudio() -> i32 {
    note("detect_subaudio");
    stub("detect_subaudio", "answers from the RF config's per-frequency tone index, else -1");
    state::with(|s| {
        if !s.squelch_open() {
            return -1;
        }
        s.rf.tone_at(s.rx_freq_hz).unwrap_or(-1)
    })
}

extern "C" fn api_set_subaudio_scan_filter(on: bool) {
    note("set_subaudio_scan_filter");
    state::with(|s| s.subaudio_scan_filter = on);
}

// ---------------------------------------------------------------- handoff

extern "C" fn api_handoff_write(buf: *const u8, len: u16) -> bool {
    note("handoff_write");
    if buf.is_null() || len as usize > 64 {
        return false;
    }
    state::with(|s| {
        let slice = unsafe { std::slice::from_raw_parts(buf, len as usize) };
        s.handoff[..len as usize].copy_from_slice(slice);
        s.handoff_len = len;
    });
    true
}

extern "C" fn api_handoff_read(buf: *mut u8, len: u16) -> u16 {
    note("handoff_read");
    if buf.is_null() {
        return 0;
    }
    state::with(|s| {
        let n = s.handoff_len.min(len) as usize;
        unsafe { std::ptr::copy_nonoverlapping(s.handoff.as_ptr(), buf, n) };
        n as u16
    })
}

// ---------------------------------------------------------------- clock

extern "C" fn api_utc_get() -> u32 {
    note("utc_get");
    state::with(|s| s.utc_now())
}

extern "C" fn api_utc_set(secs: u32) {
    note("utc_set");
    state::with(|s| s.set_utc(secs));
}

// ---------------------------------------------------------------- search/scan hardware

extern "C" fn api_freq_scan_enable() {
    note("freq_scan_enable");
    stub("freq_scan_enable", "no FD6818 measurement engine; check_freq_scan reports the strongest modelled signal near the tuned frequency");
    state::with(|s| s.freq_scanning = true);
}

extern "C" fn api_freq_scan_disable() {
    note("freq_scan_disable");
    state::with(|s| s.freq_scanning = false);
}

extern "C" fn api_check_freq_scan() -> u32 {
    note("check_freq_scan");
    state::with(|s| {
        if !s.freq_scanning {
            return u32::MAX;
        }
        // Strongest modelled signal within +/-2 MHz that clears squelch.
        let mut best: Option<(u32, i32)> = None;
        for &(hz, dbm, _) in &s.rf.levels {
            if hz.abs_diff(s.rx_freq_hz) <= 2_000_000 && dbm >= s.rf.sql_open_dbm {
                if best.is_none_or(|(_, b)| dbm > b) {
                    best = Some((hz, dbm));
                }
            }
        }
        best.map(|(hz, _)| hz).unwrap_or(u32::MAX)
    })
}

extern "C" fn api_correct_measured_freq_word(raw_word: u32) -> u32 {
    note("correct_measured_freq_word");
    stub("correct_measured_freq_word", "identity (no calibration model)");
    raw_word
}

extern "C" fn api_tune_search_candidate(freq_hz: u32, _uhf_path: bool) {
    note("tune_search_candidate");
    state::with(|s| s.rx_freq_hz = freq_hz);
}

// ---------------------------------------------------------------- fault

extern "C" fn api_app_fault(line: u32) -> ! {
    note("app_fault");
    eprintln!("[sim] FATAL: app_fault({line}) — on the radio this resets the MCU. Aborting the run.");
    std::process::exit(70);
}

// ---------------------------------------------------------------- the vtable

pub fn make_api() -> Api {
    Api {
        uptime_100us: api_uptime_100us,
        delay_ms: api_delay_ms,
        fill_rect: api_fill_rect,
        blit: api_blit,
        draw_text: api_draw_text,
        draw_line: api_draw_line,
        draw_circle: api_draw_circle,
        set_rx_freq: api_set_rx_freq,
        set_tx_freq: api_set_tx_freq,
        set_ptt: api_set_ptt,
        read_rssi: api_read_rssi,
        set_modulation: api_set_modulation,
        set_power: api_set_power,
        set_subaudio_tx: api_set_subaudio_tx,
        set_subaudio_rx: api_set_subaudio_rx,
        tone_on: api_tone_on,
        tone_off: api_tone_off,
        set_speaker: api_set_speaker,
        read_mic_level: api_read_mic_level,
        nor_read: api_nor_read,
        nor_write: api_nor_write,
        nor_erase_sector: api_nor_erase_sector,
        fmt_u32: api_fmt_u32,
        fmt_freq: api_fmt_freq,
        fm_tune_khz: api_fm_tune_khz,
        fm_seek: api_fm_seek,
        fm_status: api_fm_status,
        fm_tuned_freq_khz: api_fm_tuned_freq_khz,
        fm_power_off: api_fm_power_off,
        fm_channels_load: api_fm_channels_load,
        fm_channels_save: api_fm_channels_save,
        chan_read: api_chan_read,
        settings_get: api_settings_get,
        app_fault: api_app_fault,
        chan_write: api_chan_write,
        get_master_freq: api_get_master_freq,
        draw_list: api_draw_list,
        subaudio_index_of_code: api_subaudio_index_of_code,
        subaudio_code_of_index: api_subaudio_code_of_index,
        subaudio_format: api_subaudio_format,
        handoff_write: api_handoff_write,
        handoff_read: api_handoff_read,
        set_bandwidth: api_set_bandwidth,
        set_sql_level: api_set_sql_level,
        set_monitor: api_set_monitor,
        enter_rx: api_enter_rx,
        retune_rx: api_retune_rx,
        read_rf_gains: api_read_rf_gains,
        adjust_rf_gain: api_adjust_rf_gain,
        tx_state: api_tx_state,
        set_tx_enabled: api_set_tx_enabled,
        utc_get: api_utc_get,
        utc_set: api_utc_set,
        set_backlight_hold: api_set_backlight_hold,
        get_master_wide: api_get_master_wide,
        squelch_open: api_squelch_open,
        set_subaudio_scan_filter: api_set_subaudio_scan_filter,
        detect_subaudio: api_detect_subaudio,
        save_master_subaudio: api_save_master_subaudio,
        freq_scan_enable: api_freq_scan_enable,
        freq_scan_disable: api_freq_scan_disable,
        check_freq_scan: api_check_freq_scan,
        correct_measured_freq_word: api_correct_measured_freq_word,
        tune_search_candidate: api_tune_search_candidate,
        save_master_vfo: api_save_master_vfo,
        settings_read: api_settings_read,
        settings_preview: api_settings_preview,
        settings_commit: api_settings_commit,
        side_cfg_read: api_side_cfg_read,
        side_cfg_write: api_side_cfg_write,
        factory_reset: api_factory_reset,
        battery_raw12_avg: api_battery_raw12_avg,
        set_holds_rx: api_set_holds_rx,
        set_af_out: api_set_af_out,
        set_agc_fix: api_set_agc_fix,
    }
}

// Keep the Mutex import used even if stub set logic changes.
#[allow(dead_code)]
fn _unused(_: &Mutex<()>) {}
