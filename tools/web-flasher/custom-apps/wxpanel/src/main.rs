//! WXPANEL — NOAA weather radio panel.
//!
//! Lists the 7 NOAA WX channels, samples RSSI on each, and tunes the
//! strongest. UP/DOWN + MENU tune manually, `*` re-runs the auto-scan.
//! Read-only: never writes flash, never transmits.

#![no_std]
#![no_main]

use bf5rh_abi::{Api, AppEvent, AppResult, ListRow};
use core::fmt::Write as _;

const KIND_SINGLE: u8 = 2;
const KIND_REPEAT: u8 = 4;
const KEY_ASTERISK: u8 = 10;
const KEY_MENU: u8 = 12;
const KEY_EXIT: u8 = 13;
const KEY_UP: u8 = 14;
const KEY_DOWN: u8 = 15;

const F6: u8 = 1;
const BLACK: u16 = 0x0000;
const WHITE: u16 = 0xFFFF;

const WX: [(&str, u32); 7] = [
    ("WX1", 162_550_000),
    ("WX2", 162_400_000),
    ("WX3", 162_475_000),
    ("WX4", 162_425_000),
    ("WX5", 162_450_000),
    ("WX6", 162_500_000),
    ("WX7", 162_525_000),
];

const SAMPLE_SETTLE_100US: u32 = 600; // 60 ms after retune
const SAMPLE_GAP_100US: u32 = 100; // 10 ms between samples
const SAMPLES_PER_CH: u8 = 3;
const LIVE_REFRESH_100US: u32 = 3000; // 300 ms

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Sampling,
    Listen,
}

struct State {
    phase: Phase,
    wide: bool,
    // sampling sub-state
    samp_idx: usize,
    samp_wait_100us: u32,
    samp_acc: u32,
    samp_n: u8,
    // results: dBm per channel, i16::MIN = unknown
    dbm: [i16; 7],
    selected: usize,
    tuned: usize,
    live_dbm: i16,
    live_wait_100us: u32,
}

static mut STATE: State = State {
    phase: Phase::Sampling,
    wide: true,
    samp_idx: 0,
    samp_wait_100us: 0,
    samp_acc: 0,
    samp_n: 0,
    dbm: [i16::MIN; 7],
    selected: 0,
    tuned: 0,
    live_dbm: i16::MIN,
    live_wait_100us: 0,
};

static mut API_PTR: *const Api = core::ptr::null();

#[derive(Clone, Copy, PartialEq, Eq)]
struct Snap {
    phase: Phase,
    samp_idx: usize,
    dbm: [i16; 7],
    selected: usize,
    tuned: usize,
    live_dbm: i16,
}

static mut LAST_SNAP: Option<Snap> = None;

fn dbm_of(raw: u16) -> i16 {
    raw as i16 - 170
}

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

fn start_sampling(api: &Api, st: &mut State) {
    st.phase = Phase::Sampling;
    st.samp_idx = 0;
    st.samp_wait_100us = 0;
    st.samp_acc = 0;
    st.samp_n = 0;
    st.dbm = [i16::MIN; 7];
    (api.set_af_out)(false);
    (api.retune_rx)(WX[0].1, st.wide);
}

fn tune(api: &Api, st: &mut State, idx: usize) {
    st.tuned = idx;
    st.selected = idx;
    st.phase = Phase::Listen;
    st.live_dbm = i16::MIN;
    st.live_wait_100us = 0;
    (api.set_rx_freq)(WX[idx].1);
    (api.set_tx_freq)(WX[idx].1);
    (api.enter_rx)();
    (api.set_af_out)(true);
}

fn enter(api: &Api) -> AppResult {
    let wide = (api.get_master_wide)();
    (api.set_modulation)(0); // FM
    (api.set_bandwidth)(wide);
    (api.set_subaudio_rx)(0);
    (api.set_sql_level)((api.settings_get)(0) as u8);
    (api.set_rx_freq)(WX[0].1);
    (api.set_tx_freq)(WX[0].1);
    (api.enter_rx)();

    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    st.wide = wide;
    start_sampling(api, st);
    unsafe {
        LAST_SNAP = None;
    }
    AppResult::Continue
}

fn finish_sampling(api: &Api, st: &mut State) {
    // strongest = highest dBm among measured channels
    let mut best = 0usize;
    for i in 1..7 {
        if st.dbm[i] > st.dbm[best] {
            best = i;
        }
    }
    tune(api, st, best);
}

fn tick(api: &Api, dt_100us: u32) -> AppResult {
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match st.phase {
        Phase::Sampling => {
            st.samp_wait_100us += dt_100us;
            let target = if st.samp_n == 0 {
                SAMPLE_SETTLE_100US
            } else {
                SAMPLE_GAP_100US
            };
            if st.samp_wait_100us >= target {
                st.samp_wait_100us = 0;
                st.samp_acc += (api.read_rssi)() as u32;
                st.samp_n += 1;
                if st.samp_n >= SAMPLES_PER_CH {
                    let raw = (st.samp_acc / SAMPLES_PER_CH as u32) as u16;
                    st.dbm[st.samp_idx] = dbm_of(raw);
                    st.samp_acc = 0;
                    st.samp_n = 0;
                    if st.samp_idx + 1 >= 7 {
                        finish_sampling(api, st);
                    } else {
                        st.samp_idx += 1;
                        (api.retune_rx)(WX[st.samp_idx].1, st.wide);
                    }
                }
            }
        }
        Phase::Listen => {
            st.live_wait_100us += dt_100us;
            if st.live_wait_100us >= LIVE_REFRESH_100US {
                st.live_wait_100us = 0;
                st.live_dbm = dbm_of((api.read_rssi)());
            }
        }
    }
    AppResult::Continue
}

fn key(api: &Api, id: u8, kind: u8) -> AppResult {
    if kind != KIND_SINGLE && kind != KIND_REPEAT {
        return AppResult::Continue;
    }
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match id {
        KEY_EXIT => {
            (api.set_af_out)(true);
            return AppResult::Exit;
        }
        KEY_UP => {
            if st.phase == Phase::Listen {
                st.selected = if st.selected == 0 { 6 } else { st.selected - 1 };
            }
        }
        KEY_DOWN => {
            if st.phase == Phase::Listen {
                st.selected = (st.selected + 1) % 7;
            }
        }
        KEY_MENU => {
            if kind == KIND_SINGLE && st.phase == Phase::Listen {
                let idx = st.selected;
                tune(api, st, idx);
            }
        }
        KEY_ASTERISK => {
            if kind == KIND_SINGLE {
                start_sampling(api, st);
            }
        }
        _ => {}
    }
    AppResult::Continue
}

struct TBuf {
    b: [u8; 24],
    n: usize,
}

impl TBuf {
    fn new() -> Self {
        TBuf { b: [0; 24], n: 0 }
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

fn draw(api: &Api) -> AppResult {
    static mut ROWS: [ListRow; 8] = [ListRow {
        label: [0; 16],
        value: [0; 18],
        has_value: false,
        cursor: -1,
    }; 8];

    let st = unsafe { &*core::ptr::addr_of!(STATE) };
    let snap = Snap {
        phase: st.phase,
        samp_idx: st.samp_idx,
        dbm: st.dbm,
        selected: st.selected,
        tuned: st.tuned,
        live_dbm: st.live_dbm,
    };
    if unsafe { LAST_SNAP } == Some(snap) {
        return AppResult::Continue;
    }
    unsafe {
        LAST_SNAP = Some(snap);
    }

    let rows = unsafe { &mut *core::ptr::addr_of_mut!(ROWS) };
    for i in 0..7 {
        let mut label = TBuf::new();
        let mut fbuf = [0u8; 12];
        let flen = (api.fmt_freq)(WX[i].1, fbuf.as_mut_ptr()) as usize;
        let fstr = core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("");
        let _ = write!(label, "{} {}", WX[i].0, fstr);

        let mut value = TBuf::new();
        if st.phase == Phase::Sampling && i == st.samp_idx {
            let _ = write!(value, "...");
        } else if st.dbm[i] != i16::MIN {
            let d = if st.phase == Phase::Listen && i == st.tuned && st.live_dbm != i16::MIN {
                st.live_dbm
            } else {
                st.dbm[i]
            };
            let _ = write!(value, "{} dBm", d);
            if i == st.tuned && st.phase == Phase::Listen {
                let _ = write!(value, " *");
            }
        }

        let mut row = ListRow {
            label: [0; 16],
            value: [0; 18],
            has_value: false,
            cursor: -1,
        };
        let ll = label.as_slice();
        row.label[..ll.len().min(16)].copy_from_slice(&ll[..ll.len().min(16)]);
        let vl = value.as_slice();
        row.value[..vl.len().min(18)].copy_from_slice(&vl[..vl.len().min(18)]);
        row.has_value = !vl.is_empty();
        rows[i] = row;
    }
    rows[7] = ListRow {
        label: [0; 16],
        value: [0; 18],
        has_value: false,
        cursor: -1,
    };

    let (title, selected) = match st.phase {
        Phase::Sampling => (b"WX SCANNING..." as &[u8], st.samp_idx),
        Phase::Listen => (b"NOAA WX PANEL" as &[u8], st.selected),
    };
    (api.draw_list)(
        title.as_ptr(),
        title.len() as u16,
        rows.as_ptr(),
        7,
        0,
        selected as u16,
        7,
        false,
    );
    let _ = (F6, BLACK, WHITE);
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
