//! FOXHUNT — direction-finding helper.
//!
//! Big live RSSI readout (dBm = raw - 170, the firmware's own conversion)
//! with a bar graph, peak-hold marker, up to 8 bearing samples
//! (MENU = mark), an optional beeper whose rate follows signal strength
//! (# toggles), and stepped guidance text. `*` clears samples + peak.
//! UP/DOWN step the frequency by 12.5 kHz. Never transmits.

#![no_std]
#![no_main]

use bf5rh_abi::{Api, AppEvent, AppResult};
use core::fmt::Write as _;

const KIND_SINGLE: u8 = 2;
const KIND_REPEAT: u8 = 4;
const KEY_ASTERISK: u8 = 10;
const KEY_POUND: u8 = 11;
const KEY_MENU: u8 = 12;
const KEY_EXIT: u8 = 13;
const KEY_UP: u8 = 14;
const KEY_DOWN: u8 = 15;

const BLACK: u16 = 0x0000;
const WHITE: u16 = 0xFFFF;
const TITLE_BG: u16 = 0x0881;
const GRAY: u16 = 0x4208;
const GREEN: u16 = 0x07E0;
const YELLOW: u16 = 0xFFE0;
const RED: u16 = 0xF800;

const F6: u8 = 1; // 6x10
const F9: u8 = 2; // 9x18

const SCREEN_W: i16 = 160;

const RSSI_POLL_100US: u32 = 500; // 50 ms
const DISPLAY_100US: u32 = 2000; // 200 ms redraw cadence
const PEAK_DECAY_100US: u32 = 20_000; // peak -1 dB per 2 s
const TONE_DUR_100US: u32 = 600; // 60 ms beep
const BEEP_MIN_DBM: i32 = -128;
const MAX_SAMPLES: usize = 8;

#[derive(Clone, Copy)]
struct Sample {
    dbm: i16,
    utc: u32,
    elapsed_s: u32,
}

struct State {
    freq: u32,
    wide: bool,
    cur_dbm: i32,
    shown_dbm: i32,
    peak_dbm: i32,
    rssi_wait_100us: u32,
    disp_wait_100us: u32,
    peak_wait_100us: u32,
    samples: [Sample; MAX_SAMPLES],
    sample_count: usize,
    beeper: bool,
    beep_wait_100us: u32,
    tone_left_100us: u32,
    start_100us: u32,
    dirty: bool,
}

static mut STATE: State = State {
    freq: 0,
    wide: true,
    cur_dbm: -170,
    shown_dbm: -170,
    peak_dbm: -170,
    rssi_wait_100us: 0,
    disp_wait_100us: 0,
    peak_wait_100us: 0,
    samples: [Sample {
        dbm: 0,
        utc: 0,
        elapsed_s: 0,
    }; MAX_SAMPLES],
    sample_count: 0,
    beeper: true,
    beep_wait_100us: 0,
    tone_left_100us: 0,
    start_100us: 0,
    dirty: true,
};

static mut API_PTR: *const Api = core::ptr::null();

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

fn enter(api: &Api) -> AppResult {
    let freq = (api.get_master_freq)();
    let wide = (api.get_master_wide)();
    (api.set_modulation)(0); // FM
    (api.set_rx_freq)(freq);
    (api.set_tx_freq)(freq);
    (api.set_subaudio_rx)(0);
    (api.set_bandwidth)(wide);
    (api.set_sql_level)((api.settings_get)(0) as u8);
    (api.enter_rx)();

    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    st.freq = freq;
    st.wide = wide;
    st.cur_dbm = -170;
    st.shown_dbm = -170;
    st.peak_dbm = -170;
    st.sample_count = 0;
    st.beeper = true;
    st.beep_wait_100us = 0;
    st.tone_left_100us = 0;
    st.start_100us = (api.uptime_100us)();
    st.dirty = true;
    AppResult::Continue
}

fn beep_interval_100us(dbm: i32) -> u32 {
    // -128 dBm -> 1.5 s, -45 dBm -> 120 ms, linear in between
    let span = (-45 - BEEP_MIN_DBM) as u32; // 83
    let over = (dbm - BEEP_MIN_DBM).clamp(0, span as i32) as u32;
    15_000 - (over * (15_000 - 1_200)) / span
}

fn tick(api: &Api, dt_100us: u32) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };

    st.rssi_wait_100us += dt_100us;
    if st.rssi_wait_100us >= RSSI_POLL_100US {
        st.rssi_wait_100us = 0;
        st.cur_dbm = (api.read_rssi)() as i32 - 170;
        if st.cur_dbm > st.peak_dbm {
            st.peak_dbm = st.cur_dbm;
            st.peak_wait_100us = 0;
        }
    }

    st.peak_wait_100us += dt_100us;
    if st.peak_wait_100us >= PEAK_DECAY_100US {
        st.peak_wait_100us = 0;
        if st.peak_dbm > st.cur_dbm {
            st.peak_dbm -= 1;
        }
    }

    // beeper
    if st.tone_left_100us > 0 {
        st.tone_left_100us = st.tone_left_100us.saturating_sub(dt_100us);
        if st.tone_left_100us == 0 {
            (api.tone_off)();
        }
    } else if st.beeper && st.cur_dbm > BEEP_MIN_DBM {
        st.beep_wait_100us += dt_100us;
        if st.beep_wait_100us >= beep_interval_100us(st.cur_dbm) {
            st.beep_wait_100us = 0;
            (api.tone_on)(120); // 1200 Hz
            st.tone_left_100us = TONE_DUR_100US;
        }
    } else {
        st.beep_wait_100us = 0;
    }

    st.disp_wait_100us += dt_100us;
    if st.disp_wait_100us >= DISPLAY_100US {
        st.disp_wait_100us = 0;
        st.shown_dbm = st.cur_dbm;
        st.dirty = true;
    }
    AppResult::Continue
}

fn key(api: &Api, id: u8, kind: u8) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match (id, kind) {
        (KEY_EXIT, KIND_SINGLE) => {
            (api.tone_off)();
            return AppResult::Exit;
        }
        (KEY_MENU, KIND_SINGLE) => {
            if st.sample_count < MAX_SAMPLES {
                let now = (api.uptime_100us)();
                let s = Sample {
                    dbm: st.cur_dbm as i16,
                    utc: (api.utc_get)(),
                    elapsed_s: now.wrapping_sub(st.start_100us) / 10_000,
                };
                st.samples[st.sample_count] = s;
                st.sample_count += 1;
            }
            st.dirty = true;
        }
        (KEY_ASTERISK, KIND_SINGLE) => {
            st.sample_count = 0;
            st.peak_dbm = st.cur_dbm;
            st.dirty = true;
        }
        (KEY_POUND, KIND_SINGLE) => {
            st.beeper = !st.beeper;
            if !st.beeper {
                (api.tone_off)();
                st.tone_left_100us = 0;
            }
            st.dirty = true;
        }
        (KEY_UP, k) if k == KIND_SINGLE || k == KIND_REPEAT => {
            st.freq = st.freq.saturating_add(12_500);
            (api.retune_rx)(st.freq, st.wide);
            st.peak_dbm = st.cur_dbm;
            st.dirty = true;
        }
        (KEY_DOWN, k) if k == KIND_SINGLE || k == KIND_REPEAT => {
            st.freq = st.freq.saturating_sub(12_500).max(1_000_000);
            (api.retune_rx)(st.freq, st.wide);
            st.peak_dbm = st.cur_dbm;
            st.dirty = true;
        }
        _ => {}
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

fn bar_x(dbm: i32) -> i16 {
    // -130 .. -40 dBm -> 2 .. 158 px
    let over = (dbm + 130).clamp(0, 90) as i16;
    2 + (over * 156) / 90
}

fn guidance(dbm: i32, peak: i32) -> &'static [u8] {
    if dbm >= -60 {
        b"VERY STRONG: body-block it"
    } else if dbm >= peak - 3 && dbm > -95 {
        b"STRONG: add ATTN, slow sweep"
    } else if dbm >= -95 {
        b"WARM: sweep slowly, mark"
    } else if dbm >= -115 {
        b"WEAK: raise ant, no ATTN"
    } else {
        b"NO SIGNAL: keep moving"
    }
}

fn draw(api: &Api) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if !st.dirty {
        return AppResult::Continue;
    }
    st.dirty = false;

    (api.fill_rect)(0, 0, SCREEN_W as u16, 128, BLACK);
    (api.fill_rect)(0, 0, SCREEN_W as u16, 14, TITLE_BG);
    text(api, 4, 2, b"FOXHUNT", WHITE, TITLE_BG, F6);
    let mut t = TBuf::new();
    let _ = write!(t, "BEEP {}", if st.beeper { "ON" } else { "OFF" });
    text(api, 110, 2, t.as_slice(), WHITE, TITLE_BG, F6);

    // frequency
    let mut fbuf = [0u8; 12];
    let flen = (api.fmt_freq)(st.freq, fbuf.as_mut_ptr()) as usize;
    text(
        api,
        4,
        18,
        &fbuf[..flen.min(12)],
        WHITE,
        BLACK,
        F9,
    );

    // big dBm + peak
    let mut t = TBuf::new();
    let _ = write!(t, "{} dBm", st.shown_dbm);
    text(api, 4, 42, t.as_slice(), WHITE, BLACK, F9);
    let mut t = TBuf::new();
    let _ = write!(t, "PK {}", st.peak_dbm);
    text(api, 104, 42, t.as_slice(), WHITE, BLACK, F6);

    // bar
    (api.fill_rect)(2, 68, 156, 10, GRAY);
    let x = bar_x(st.shown_dbm);
    let color = if st.shown_dbm > -55 {
        RED
    } else if st.shown_dbm > -75 {
        YELLOW
    } else {
        GREEN
    };
    if x > 2 {
        (api.fill_rect)(2, 68, (x - 2) as u16, 10, color);
    }
    let px = bar_x(st.peak_dbm);
    (api.draw_line)(px, 66, px, 80, WHITE);

    // guidance
    text(
        api,
        4,
        84,
        guidance(st.shown_dbm, st.peak_dbm),
        WHITE,
        BLACK,
        F6,
    );

    // last two samples
    let mut y = 98;
    let from = st.sample_count.saturating_sub(2);
    for i in from..st.sample_count {
        let s = st.samples[i];
        let mut t = TBuf::new();
        if s.utc != 0 {
            let _ = write!(
                t,
                "#{} {}dBm {:02}:{:02}:{:02}",
                i + 1,
                s.dbm,
                (s.utc / 3600) % 24,
                (s.utc / 60) % 60,
                s.utc % 60
            );
        } else {
            let _ = write!(t, "#{} {}dBm T+{}s", i + 1, s.dbm, s.elapsed_s);
        }
        text(api, 4, y, t.as_slice(), WHITE, BLACK, F6);
        y += 11;
    }

    text(api, 4, 118, b"MENU mark #beep *clr", WHITE, BLACK, F6);
    AppResult::Continue
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
