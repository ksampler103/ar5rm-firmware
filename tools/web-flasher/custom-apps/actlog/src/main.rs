//! ACTLOG — activity logger.
//!
//! Scans the radio's programmed channels (via ABI `chan_read`; falls
//! back to a fixed NOAA/GMRS/simplex list when no channels exist) and
//! logs every squelch-open "hit": frequency, start time (UTC when the
//! radio clock is set, else elapsed), peak RSSI, and duration.
//!
//! PERSISTENCE — the log lives in the app-private NOR data region
//! (flash >= OVERLAY_DATA_ADDR 0x18C000; `nor_write`/`nor_erase_sector`
//! refuse anything below the overlay area, and `nor_read` is
//! unrestricted). Layout, chosen clear of the satellite app's region
//! (0x18C000..0x191000):
//!   0x191000  header sector: magic "ALOG", version byte
//!   0x192000..0x195000  log area: 3 sectors, append-only 16-byte
//!             records; the first all-0xFF record marks the end, so no
//!             header counter ever needs an erase to update.
//!   Record (16 B, LE): freq_hz u32, start_utc u32 (0 = clock unset),
//!             dur_ms u16, peak_raw u16 (RSSI raw; dBm = raw - 170),
//!             chan u16 (0-based channel no., 0xFFFF = fixed list),
//!             kind u8 (0 = channel, 1 = fixed), pad u8, reserved u16.
//!   Capacity: 768 hits. Survives app exit and power cycles. Erasing
//!   happens only on first run (bad magic) or an explicit clear
//!   (in the review view: `*`, then MENU to confirm).
//!
//! Scanning notes: ~90 ms worst-case dwell per channel, carrier detect
//! is tone-agnostic (`set_subaudio_rx(0)`), speaker is muted while the
//! app runs (`set_af_out(false)`, restored on exit). MENU switches to
//! the review list (scanning pauses); EXIT leaves.

#![no_std]
#![no_main]

use bf5rh_abi::{Api, AppEvent, AppResult, ListRow};
use core::fmt::Write as _;

const KIND_SINGLE: u8 = 2;
const KEY_ASTERISK: u8 = 10;
const KEY_MENU: u8 = 12;
const KEY_EXIT: u8 = 13;
const KEY_UP: u8 = 14;
const KEY_DOWN: u8 = 15;

const BLACK: u16 = 0x0000;
const WHITE: u16 = 0xFFFF;
const TITLE_BG: u16 = 0x0881;
const F6: u8 = 1;
const F9: u8 = 2;

// --- persistent log layout (see header) ---
const HDR_ADDR: u32 = 0x191000;
const LOG_ADDR: u32 = 0x192000;
const LOG_SECTORS: u32 = 3;
const SECTOR: u32 = 0x1000;
const REC_SIZE: u32 = 16;
const MAX_HITS: u32 = (LOG_SECTORS * SECTOR) / REC_SIZE; // 768
const LOG_VERSION: u8 = 1;

const MAX_ENTRIES: usize = 192;
const REVIEW_MAX: usize = 48;

const SETTLE_100US: u32 = 300; // 30 ms before first squelch check
const DWELL_100US: u32 = 900; // 90 ms worst-case dwell
const CLOSE_100US: u32 = 3000; // 300 ms of silence ends a hit
const MIN_HIT_100US: u32 = 2000; // ignore openings < 200 ms

#[derive(Clone, Copy)]
struct Entry {
    freq: u32,
    chan: u16, // 0-based, 0xFFFF = fixed list
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Dwell,
    Hit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Scan,
    Review,
    ConfirmClear,
}

struct State {
    entries: [Entry; MAX_ENTRIES],
    entry_count: usize,
    idx: usize,
    phase: Phase,
    view: View,
    dwell_100us: u32,
    // hit accumulator
    hit_start_utc: u32,
    hit_open_100us: u32,
    hit_close_100us: u32,
    hit_peak_raw: u16,
    // persistent log
    log_count: u32,
    log_full: bool,
    // review cache: raw 16-byte records, newest first
    review: [[u8; REC_SIZE as usize]; REVIEW_MAX],
    review_count: usize,
    review_sel: usize,
    // display bookkeeping
    wide: bool,
    start_100us: u32,
    last_freq: u32,
    last_dbm: i32,
    last_dur_s: u32,
    have_last: bool,
    dirty: bool,
    disp_wait_100us: u32,
}

static mut STATE: State = State {
    entries: [Entry {
        freq: 0,
        chan: 0xFFFF,
    }; MAX_ENTRIES],
    entry_count: 0,
    idx: 0,
    phase: Phase::Dwell,
    view: View::Scan,
    dwell_100us: 0,
    hit_start_utc: 0,
    hit_open_100us: 0,
    hit_close_100us: 0,
    hit_peak_raw: 0,
    log_count: 0,
    log_full: false,
    review: [[0xFF; REC_SIZE as usize]; REVIEW_MAX],
    review_count: 0,
    review_sel: 0,
    wide: true,
    start_100us: 0,
    last_freq: 0,
    last_dbm: 0,
    last_dur_s: 0,
    have_last: false,
    dirty: true,
    disp_wait_100us: 0,
};

static mut API_PTR: *const Api = core::ptr::null();

const FIXED_LIST: &[u32] = &[
    162_550_000,
    162_400_000,
    162_475_000,
    162_425_000,
    162_450_000,
    162_500_000,
    162_525_000, // NOAA WX1..WX7
    462_550_000,
    462_575_000,
    462_600_000,
    462_625_000,
    462_650_000,
    462_675_000,
    462_700_000,
    462_725_000, // GMRS repeater outputs / main channels
    146_520_000, // 2 m simplex calling
    446_000_000, // 70 cm simplex calling
];

fn bcd4_to_hz(b: &[u8]) -> u32 {
    let dec = |x: u8| -> u32 { ((x >> 4) * 10 + (x & 0x0F)) as u32 };
    (dec(b[3]) * 1_000_000 + dec(b[2]) * 10_000 + dec(b[1]) * 100 + dec(b[0])) * 10
}

fn build_entries(api: &Api, st: &mut State) {
    let mut raw = [0u8; 32];
    let mut n = 0usize;
    for num in 0..1024u16 {
        if n >= MAX_ENTRIES {
            break;
        }
        if !(api.chan_read)(num, raw.as_mut_ptr(), 32) {
            continue;
        }
        let freq = bcd4_to_hz(&raw[0..4]);
        if freq == 0 {
            continue;
        }
        // dedupe by frequency (the owner's plan repeats several)
        if st.entries[..n].iter().any(|e| e.freq == freq) {
            continue;
        }
        st.entries[n] = Entry { freq, chan: num };
        n += 1;
    }
    if n == 0 {
        for &f in FIXED_LIST {
            st.entries[n] = Entry {
                freq: f,
                chan: 0xFFFF,
            };
            n += 1;
        }
    }
    st.entry_count = n;
}

// --- persistent log ---

fn log_init(api: &Api, st: &mut State) {
    let mut hdr = [0u8; 8];
    (api.nor_read)(HDR_ADDR, hdr.as_mut_ptr(), 8);
    if &hdr[0..4] != b"ALOG" || hdr[4] != LOG_VERSION {
        for s in 0..(1 + LOG_SECTORS) {
            (api.nor_erase_sector)(HDR_ADDR + s * SECTOR);
        }
        let mut h = [0xFFu8; 8];
        h[0..4].copy_from_slice(b"ALOG");
        h[4] = LOG_VERSION;
        (api.nor_write)(HDR_ADDR, h.as_ptr(), 8);
        st.log_count = 0;
        return;
    }
    // count = index of first all-0xFF record
    let mut rec = [0u8; REC_SIZE as usize];
    let mut count = 0u32;
    while count < MAX_HITS {
        (api.nor_read)(
            LOG_ADDR + count * REC_SIZE,
            rec.as_mut_ptr(),
            REC_SIZE,
        );
        if rec.iter().all(|&b| b == 0xFF) {
            break;
        }
        count += 1;
    }
    st.log_count = count;
    st.log_full = count >= MAX_HITS;
}

fn log_append(api: &Api, st: &mut State, freq: u32, chan: u16, utc: u32, dur_ms: u16, peak_raw: u16) {
    if st.log_full {
        return;
    }
    let mut rec = [0u8; REC_SIZE as usize];
    rec[0..4].copy_from_slice(&freq.to_le_bytes());
    rec[4..8].copy_from_slice(&utc.to_le_bytes());
    rec[8..10].copy_from_slice(&dur_ms.to_le_bytes());
    rec[10..12].copy_from_slice(&peak_raw.to_le_bytes());
    rec[12..14].copy_from_slice(&chan.to_le_bytes());
    rec[14] = if chan == 0xFFFF { 1 } else { 0 };
    let addr = LOG_ADDR + st.log_count * REC_SIZE;
    if (api.nor_write)(addr, rec.as_ptr(), REC_SIZE) {
        st.log_count += 1;
        if st.log_count >= MAX_HITS {
            st.log_full = true;
        }
    } else {
        st.log_full = true;
    }
}

fn log_clear(api: &Api, st: &mut State) {
    for s in 0..(1 + LOG_SECTORS) {
        (api.nor_erase_sector)(HDR_ADDR + s * SECTOR);
    }
    let mut h = [0xFFu8; 8];
    h[0..4].copy_from_slice(b"ALOG");
    h[4] = LOG_VERSION;
    (api.nor_write)(HDR_ADDR, h.as_ptr(), 8);
    st.log_count = 0;
    st.log_full = false;
    st.have_last = false;
}

fn load_review(api: &Api, st: &mut State) {
    let want = (st.log_count as usize).min(REVIEW_MAX);
    for i in 0..want {
        let src = st.log_count - 1 - i as u32;
        (api.nor_read)(
            LOG_ADDR + src * REC_SIZE,
            st.review[i].as_mut_ptr(),
            REC_SIZE,
        );
    }
    st.review_count = want;
    st.review_sel = 0;
}

// --- app ---

#[no_mangle]
pub extern "C" fn app_entry(api: &Api, ev: AppEvent) -> AppResult {
    unsafe {
        API_PTR = api as *const Api;
    }
    match ev {
        AppEvent::Enter => enter(api),
        AppEvent::Key { id, kind } => key(api, id, kind),
        AppEvent::Tick { dt_100us } => tick(api, dt_100us),
        AppEvent::Draw => draw(api),
        AppEvent::Leave => AppResult::Continue,
    }
}

fn tune_current(api: &Api, st: &State, first: bool) {
    let e = st.entries[st.idx];
    if first {
        (api.set_rx_freq)(e.freq);
        (api.set_tx_freq)(e.freq);
        (api.enter_rx)();
    } else {
        (api.retune_rx)(e.freq, st.wide);
    }
}

fn enter(api: &Api) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    st.wide = (api.get_master_wide)();
    build_entries(api, st);
    log_init(api, st);
    st.idx = 0;
    st.phase = Phase::Dwell;
    st.view = View::Scan;
    st.dwell_100us = 0;
    st.have_last = false;
    st.start_100us = (api.uptime_100us)();
    st.dirty = true;

    (api.set_modulation)(0); // FM
    (api.set_bandwidth)(st.wide);
    (api.set_subaudio_rx)(0); // tone-agnostic carrier detect
    (api.set_sql_level)((api.settings_get)(0) as u8);
    (api.set_af_out)(false); // quiet while logging
    tune_current(api, st, true);
    AppResult::Continue
}

fn advance(api: &Api, st: &mut State) {
    st.idx = (st.idx + 1) % st.entry_count;
    st.dwell_100us = 0;
    st.phase = Phase::Dwell;
    tune_current(api, st, false);
}

fn finalize_hit(api: &Api, st: &mut State) {
    let dur_ms = (st.hit_open_100us / 10).min(65_535) as u16;
    let e = st.entries[st.idx];
    log_append(
        api,
        st,
        e.freq,
        e.chan,
        st.hit_start_utc,
        dur_ms,
        st.hit_peak_raw,
    );
    st.last_freq = e.freq;
    st.last_dbm = st.hit_peak_raw as i32 - 170;
    st.last_dur_s = (st.hit_open_100us / 10_000) as u32;
    st.have_last = true;
    st.dirty = true;
}

fn tick(api: &Api, dt_100us: u32) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if st.view != View::Scan || st.entry_count == 0 {
        return AppResult::Continue;
    }
    match st.phase {
        Phase::Dwell => {
            st.dwell_100us += dt_100us;
            if st.dwell_100us >= SETTLE_100US && (api.squelch_open)() {
                st.phase = Phase::Hit;
                st.hit_start_utc = (api.utc_get)();
                st.hit_open_100us = 0;
                st.hit_close_100us = 0;
                st.hit_peak_raw = (api.read_rssi)();
                st.dirty = true;
            } else if st.dwell_100us >= DWELL_100US {
                advance(api, st);
            }
        }
        Phase::Hit => {
            if (api.squelch_open)() {
                st.hit_open_100us += dt_100us;
                st.hit_close_100us = 0;
                let r = (api.read_rssi)();
                if r > st.hit_peak_raw {
                    st.hit_peak_raw = r;
                }
            } else {
                st.hit_close_100us += dt_100us;
                if st.hit_close_100us >= CLOSE_100US {
                    if st.hit_open_100us >= MIN_HIT_100US {
                        finalize_hit(api, st);
                    }
                    advance(api, st);
                }
            }
        }
    }
    st.disp_wait_100us += dt_100us;
    if st.disp_wait_100us >= 5000 {
        st.disp_wait_100us = 0;
        st.dirty = true; // refresh elapsed / current freq readout
    }
    AppResult::Continue
}

fn key(api: &Api, id: u8, kind: u8) -> AppResult {
    if kind != KIND_SINGLE {
        return AppResult::Continue;
    }
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match st.view {
        View::Scan => match id {
            KEY_EXIT => {
                (api.set_af_out)(true);
                return AppResult::Exit;
            }
            KEY_MENU => {
                load_review(api, st);
                st.view = View::Review;
                st.dirty = true;
            }
            _ => {}
        },
        View::Review => match id {
            KEY_EXIT => {
                st.view = View::Scan;
                st.dirty = true;
            }
            KEY_UP => {
                if st.review_count > 0 {
                    st.review_sel = if st.review_sel == 0 {
                        st.review_count - 1
                    } else {
                        st.review_sel - 1
                    };
                    st.dirty = true;
                }
            }
            KEY_DOWN => {
                if st.review_count > 0 {
                    st.review_sel = (st.review_sel + 1) % st.review_count;
                    st.dirty = true;
                }
            }
            KEY_ASTERISK => {
                st.view = View::ConfirmClear;
                st.dirty = true;
            }
            _ => {}
        },
        View::ConfirmClear => match id {
            KEY_MENU => {
                log_clear(api, st);
                st.review_count = 0;
                st.view = View::Scan;
                st.dirty = true;
            }
            KEY_EXIT => {
                st.view = View::Review;
                st.dirty = true;
            }
            _ => {}
        },
    }
    AppResult::Continue
}

struct TBuf {
    b: [u8; 40],
    n: usize,
}

impl TBuf {
    fn new() -> Self {
        TBuf { b: [0; 40], n: 0 }
    }
    fn as_slice(&self) -> &[u8] {
        &self.b[..self.n]
    }
}

impl core::fmt::Write for TBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &c in s.as_bytes() {
            if self.n < self.b.len() {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
        Ok(())
    }
}

fn text(api: &Api, x: i16, y: i16, s: &[u8], fg: u16, bg: u16, font: u8) {
    (api.draw_text)(x, y, s.as_ptr(), s.len() as u16, fg, bg, font);
}

fn draw(api: &Api) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match st.view {
        View::Scan => {
            if !st.dirty {
                return AppResult::Continue;
            }
            st.dirty = false;
            draw_scan(api, st);
        }
        View::Review | View::ConfirmClear => {
            if !st.dirty {
                return AppResult::Continue;
            }
            st.dirty = false;
            draw_review(api, st);
        }
    }
    AppResult::Continue
}

fn draw_scan(api: &Api, st: &State) {
    (api.fill_rect)(0, 0, 160, 128, BLACK);
    (api.fill_rect)(0, 0, 160, 14, TITLE_BG);
    text(api, 4, 2, b"ACTLOG", WHITE, TITLE_BG, F6);
    let mut t = TBuf::new();
    let _ = write!(
        t,
        "{}",
        match st.phase {
            Phase::Dwell => "SCAN",
            Phase::Hit => "HIT!",
        }
    );
    text(api, 110, 2, t.as_slice(), WHITE, TITLE_BG, F6);

    if st.entry_count > 0 {
        let e = st.entries[st.idx];
        let mut fbuf = [0u8; 12];
        let flen = (api.fmt_freq)(e.freq, fbuf.as_mut_ptr()) as usize;
        text(api, 4, 20, &fbuf[..flen.min(12)], WHITE, BLACK, F9);
        let mut t = TBuf::new();
        if e.chan != 0xFFFF {
            let _ = write!(t, "CH {} of {}", e.chan + 1, st.entry_count);
        } else {
            let _ = write!(t, "FIXED LIST {}", st.entry_count);
        }
        text(api, 4, 44, t.as_slice(), WHITE, BLACK, F6);
    }

    let mut t = TBuf::new();
    let _ = write!(t, "LOG {} / {}", st.log_count, MAX_HITS);
    if st.log_full {
        let _ = write!(t, " FULL");
    }
    text(api, 4, 60, t.as_slice(), WHITE, BLACK, F6);

    if st.have_last {
        let mut fbuf = [0u8; 12];
        let flen = (api.fmt_freq)(st.last_freq, fbuf.as_mut_ptr()) as usize;
        let mut t = TBuf::new();
        let _ = write!(
            t,
            "LAST {}",
            core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("")
        );
        text(api, 4, 74, t.as_slice(), WHITE, BLACK, F6);
        let mut t = TBuf::new();
        let _ = write!(t, "{} dBm  {} s", st.last_dbm, st.last_dur_s);
        text(api, 4, 86, t.as_slice(), WHITE, BLACK, F6);
    } else {
        text(api, 4, 74, b"NO HITS YET", WHITE, BLACK, F6);
    }

    let elapsed_s = (api.uptime_100us)()
        .wrapping_sub(st.start_100us)
        / 10_000;
    let mut t = TBuf::new();
    let _ = write!(
        t,
        "RUN {:02}:{:02}:{:02}",
        elapsed_s / 3600,
        (elapsed_s / 60) % 60,
        elapsed_s % 60
    );
    text(api, 4, 102, t.as_slice(), WHITE, BLACK, F6);
    text(api, 4, 116, b"MENU log  EXIT", WHITE, BLACK, F6);
}

fn draw_review(api: &Api, st: &State) {
    static mut ROWS: [ListRow; 8] = [ListRow {
        label: [0; 16],
        value: [0; 18],
        has_value: false,
        cursor: -1,
    }; 8];

    let rows = unsafe { &mut *core::ptr::addr_of_mut!(ROWS) };
    let top = if st.review_sel < 8 {
        0
    } else {
        st.review_sel - 7
    };
    for i in 0..8 {
        let gi = top + i;
        let mut row = ListRow {
            label: [0; 16],
            value: [0; 18],
            has_value: false,
            cursor: -1,
        };
        if gi < st.review_count {
            let rec = st.review[gi];
            let freq = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
            let utc = u32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]);
            let dur_ms = u16::from_le_bytes([rec[8], rec[9]]) as u32;
            let peak_raw = u16::from_le_bytes([rec[10], rec[11]]);
            let mut fbuf = [0u8; 12];
            let flen = (api.fmt_freq)(freq, fbuf.as_mut_ptr()) as usize;
            let fstr = core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("");
            row.label[..fstr.len().min(16)].copy_from_slice(&fstr.as_bytes()[..fstr.len().min(16)]);
            let mut val = TBuf::new();
            if utc != 0 {
                let _ = write!(
                    val,
                    "{}dB {}s {:02}:{:02}",
                    peak_raw as i32 - 170,
                    dur_ms / 1000,
                    (utc / 3600) % 24,
                    (utc / 60) % 60
                );
            } else {
                let _ = write!(val, "{}dB {}s", peak_raw as i32 - 170, dur_ms / 1000);
            }
            let vl = val.as_slice();
            row.value[..vl.len().min(18)].copy_from_slice(&vl[..vl.len().min(18)]);
            row.has_value = true;
        }
        rows[i] = row;
    }

    let title: &[u8] = match st.view {
        View::ConfirmClear => b"CLEAR LOG? MENU=YES",
        _ => b"ACTLOG HITS (*=clr)",
    };
    (api.draw_list)(
        title.as_ptr(),
        title.len() as u16,
        rows.as_ptr(),
        8,
        top as u16,
        st.review_sel as u16,
        st.review_count as u16,
        false,
    );
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        let ptr = API_PTR;
        if !ptr.is_null() {
            ((*ptr).app_fault)(0);
        }
    }
    loop {}
}
