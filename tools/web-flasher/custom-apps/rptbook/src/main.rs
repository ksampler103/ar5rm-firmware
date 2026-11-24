//! RPTBOOK — offline repeater / channel directory.
//!
//! Shows a scrollable directory of entries (name, RX freq, TX freq,
//! tones) and tunes the radio to the selected one (MENU). `#` toggles a
//! detail view. If an entry was tuned, EXIT hands the frequency to the
//! standby VFO via `save_master_vfo` before leaving. The app is
//! READ-ONLY: it never writes channel data, settings, or flash.
//!
//! ================================================================
//! INJECTED DATA BLOCK CONTRACT (for the web flasher / page tools)
//! ================================================================
//! At install time a page may patch a location-specific data block
//! into this app's flash slot, AFTER the .app package bytes, so the
//! directory can be re-targeted (e.g. by ZIP code) with NO rebuild.
//!
//! Mechanism: ABI `nor_read` can read any NOR flash address (only
//! writes are fenced to >= OVERLAY_APP_ADDR), so at startup the app
//! scans the 8 slots (base 0x14C000, stride 0x8000) for the package
//! header whose name is "RPTBOOK", then reads the block at
//! `slot_base + DATA_OFFSET` in that same slot.
//!
//!   DATA_OFFSET = 0x6000 (24,576) from the slot base.
//!   The .app package itself must stay below 0x6000 bytes for this
//!   contract to hold. Slot size is 32,768 bytes, leaving
//!   32,768 - 24,576 - 16 = 8,176 bytes for records.
//!
//!   RESIDENT-SIZE RULE: this app is interactive, so its segment must
//!   fit ARENA_SIZE (8,192 B total). The firmware loader only lets an
//!   oversized segment run transiently (it must Chain/Exit straight
//!   out of Enter; a Continue faults FAULT_OVERSIZE_CONTINUE and the
//!   app never launches). That is why the full directory lives in
//!   this block and NOT in the binary: the flasher writes a block on
//!   every install (owner-default or ZIP-built). The compiled-in
//!   table in `baked.rs` is only a small fallback for when no valid
//!   block exists.
//!
//! Block layout (all integers little-endian):
//!   offset 0:  magic "RPBK" (4 bytes)
//!   offset 4:  u8  version = 1
//!   offset 5:  u8  source: 0/1 = local (ZIP-built) data, shown as
//!              "RPTBOOK (ZIP)"; 2 = owner-default block (the owner's
//!              own channels), shown as plain "RPTBOOK"
//!   offset 6:  u16 record count N (1..=272)
//!   offset 8:  u16 CRC-16/XMODEM (poly 0x1021, init 0x0000) over the
//!              N * 30 record bytes that follow the header
//!   offset 10: 6 bytes reserved (0)
//!   offset 16: N records of 30 bytes each:
//!       +0   name: 16 bytes ASCII, space- or NUL-padded
//!       +16  rx_hz: u32 (Hz)
//!       +20  tx_hz: u32 (Hz; 0 = same as rx_hz)
//!       +24  tx_tone: u16 firmware subaudio code (see below)
//!       +26  rx_tone: u16 firmware subaudio code
//!       +28  flags: u8 — bit0 duplex (tx != rx), bit1 tx tone set,
//!                    bit2 rx tone set, bit3 RX-only (never transmit)
//!       +29  reserved (0)
//!   MAX_RECORDS = 272.
//!
//! Subaudio codes are the firmware's own `flash_map::SubaudioCode`
//! encoding (identical to CPS channel records):
//!   0        = no tone
//!   1..=105  = DCS normal, 1-based index into the standard DCS table
//!   106..=250= DCS inverted (index = code - 105)
//!   >250     = CTCSS tone in tenths of a Hz (1413 = 141.3 Hz)
//!
//! The app validates magic, version, count range, and CRC before
//! trusting the block. With no valid block it falls back to the
//! small built-in table in `baked.rs`. The owner's full channel
//! directory ships as the default block (gen_default_block.py from
//! default_channels.json, extracted from the original baked table
//! that gen_baked.py produced from the owner's CPS backup). A page
//! composing a slot image writes: package bytes at slot offset 0,
//! zeros/0xFF up to 0x6000, then the block — and installs it through
//! the existing app-install flow.
//! ================================================================

#![no_std]
#![no_main]

mod baked;

use baked::BAKED;
use bf5rh_abi::{Api, AppEvent, AppResult, ListRow};
use core::fmt::Write as _;

const KIND_SINGLE: u8 = 2;
const KIND_REPEAT: u8 = 4;
const KEY_POUND: u8 = 11;
const KEY_MENU: u8 = 12;
const KEY_EXIT: u8 = 13;
const KEY_UP: u8 = 14;
const KEY_DOWN: u8 = 15;

const BLACK: u16 = 0x0000;
const WHITE: u16 = 0xFFFF;
const TITLE_BG: u16 = 0x0881;
const F6: u8 = 1;
const F9: u8 = 2;

// --- injected block contract constants (see header) ---
const OVERLAY_APP_ADDR: u32 = 0x14C000;
const SLOT_SIZE: u32 = 0x8000;
const SLOT_COUNT: u32 = 8;
const DATA_OFFSET: u32 = 0x6000;
const BLOCK_HDR: usize = 16;
const REC_SIZE: usize = 30;
const MAX_RECORDS: u16 = 272;
const BLOCK_VERSION: u8 = 1;

const FLAG_RX_ONLY: u8 = 0x08;

#[derive(Clone, Copy)]
struct Rec {
    name: [u8; 16],
    rx: u32,
    tx: u32,
    tx_tone: u16,
    rx_tone: u16,
    flags: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Baked,
    /// `local` = ZIP-built block (header source byte 0/1); the
    /// owner-default block (source byte 2) shows the plain title.
    Injected { base: u32, count: u16, local: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    List,
    Detail,
}

struct State {
    source: Source,
    count: usize,
    selected: usize,
    view: View,
    tuned: Option<usize>,
    wide: bool,
}

static mut STATE: State = State {
    source: Source::Baked,
    count: 0,
    selected: 0,
    view: View::List,
    tuned: None,
    wide: true,
};

static mut API_PTR: *const Api = core::ptr::null();

#[derive(Clone, Copy, PartialEq, Eq)]
struct Snap {
    view: View,
    selected: usize,
    count: usize,
    source_injected: bool,
    tuned: Option<usize>,
}

static mut LAST_SNAP: Option<Snap> = None;

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Find our own slot by package name, then validate an injected block.
/// Returns the source to use for the directory.
fn detect_source(api: &Api) -> Source {
    let mut hdr = [0u8; 108];
    for slot in 0..SLOT_COUNT {
        let base = OVERLAY_APP_ADDR + slot * SLOT_SIZE;
        (api.nor_read)(base, hdr.as_mut_ptr(), hdr.len() as u32);
        if &hdr[0..4] != b"OVL1" {
            continue;
        }
        let name = &hdr[12..28];
        let end = name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(name.len());
        if &name[..end] != b"RPTBOOK" {
            continue;
        }
        // candidate slot: check for an injected block
        let data_addr = base + DATA_OFFSET;
        let mut bh = [0u8; BLOCK_HDR];
        (api.nor_read)(data_addr, bh.as_mut_ptr(), BLOCK_HDR as u32);
        if &bh[0..4] != b"RPBK" || bh[4] != BLOCK_VERSION {
            return Source::Baked;
        }
        let count = u16::from_le_bytes([bh[6], bh[7]]);
        let want_crc = u16::from_le_bytes([bh[8], bh[9]]);
        if count == 0 || count > MAX_RECORDS {
            return Source::Baked;
        }
        // CRC over the record bytes, read in chunks
        let mut crc: u16 = 0;
        let mut chunk = [0u8; 300];
        let total = count as usize * REC_SIZE;
        let mut done = 0usize;
        while done < total {
            let n = (total - done).min(chunk.len());
            (api.nor_read)(
                data_addr + BLOCK_HDR as u32 + done as u32,
                chunk.as_mut_ptr(),
                n as u32,
            );
            // fold chunk into running CRC (same algorithm as crc16_xmodem)
            for &b in &chunk[..n] {
                crc ^= (b as u16) << 8;
                for _ in 0..8 {
                    crc = if crc & 0x8000 != 0 {
                        (crc << 1) ^ 0x1021
                    } else {
                        crc << 1
                    };
                }
            }
            done += n;
        }
        if crc == want_crc {
            return Source::Injected {
                base,
                count,
                local: bh[5] != 2,
            };
        }
        return Source::Baked;
    }
    Source::Baked
}

fn record(api: &Api, st: &State, idx: usize) -> Rec {
    match st.source {
        Source::Baked => {
            let b = &BAKED[idx];
            let mut name = [b' '; 16];
            name[..12].copy_from_slice(&b.name);
            Rec {
                name,
                rx: b.rx_hz,
                tx: b.tx_hz,
                tx_tone: b.tx_tone,
                rx_tone: b.rx_tone,
                flags: b.flags,
            }
        }
        Source::Injected { base, .. } => {
            let mut raw = [0u8; REC_SIZE];
            let addr = base + DATA_OFFSET + BLOCK_HDR as u32 + (idx * REC_SIZE) as u32;
            (api.nor_read)(addr, raw.as_mut_ptr(), REC_SIZE as u32);
            let mut name = [0u8; 16];
            name.copy_from_slice(&raw[0..16]);
            Rec {
                name,
                rx: u32::from_le_bytes([raw[16], raw[17], raw[18], raw[19]]),
                tx: u32::from_le_bytes([raw[20], raw[21], raw[22], raw[23]]),
                tx_tone: u16::from_le_bytes([raw[24], raw[25]]),
                rx_tone: u16::from_le_bytes([raw[26], raw[27]]),
                flags: raw[28],
            }
        }
    }
}

fn name_trim(name: &[u8; 16]) -> &[u8] {
    let mut end = name.len();
    while end > 0 && (name[end - 1] == 0 || name[end - 1] == b' ') {
        end -= 1;
    }
    &name[..end]
}

#[no_mangle]
pub extern "C" fn app_entry(api: &Api, ev: AppEvent) -> AppResult {
    unsafe {
        API_PTR = api as *const Api;
    }
    match ev {
        AppEvent::Enter => enter(api),
        AppEvent::Key { id, kind } => key(api, id, kind),
        AppEvent::Tick { dt_100us: _ } => AppResult::Continue,
        AppEvent::Draw => draw(api),
        AppEvent::Leave => AppResult::Continue,
    }
}

fn enter(api: &Api) -> AppResult {
    let source = detect_source(api);
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    st.source = source;
    st.count = match source {
        Source::Baked => BAKED.len(),
        Source::Injected { count, .. } => count as usize,
    };
    st.selected = 0;
    st.view = View::List;
    st.tuned = None;
    st.wide = (api.get_master_wide)();
    unsafe {
        LAST_SNAP = None;
    }
    AppResult::Continue
}

fn tune(api: &Api, st: &mut State, idx: usize) {
    let rec = record(api, st, idx);
    let tx_eff = if rec.tx == 0 { rec.rx } else { rec.tx };
    (api.set_modulation)(0); // FM
    (api.set_rx_freq)(rec.rx);
    (api.set_tx_freq)(tx_eff);
    (api.set_subaudio_rx)(rec.rx_tone);
    (api.set_subaudio_tx)(rec.tx_tone);
    (api.set_bandwidth)(st.wide);
    (api.set_sql_level)((api.settings_get)(0) as u8);
    (api.enter_rx)();
    (api.set_af_out)(true);
    st.tuned = Some(idx);
}

fn key(api: &Api, id: u8, kind: u8) -> AppResult {
    if kind != KIND_SINGLE && kind != KIND_REPEAT {
        return AppResult::Continue;
    }
    let st = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    match id {
        KEY_EXIT => {
            if kind == KIND_SINGLE {
                // hand the tuned frequency to the standby VFO
                if let Some(idx) = st.tuned {
                    let rec = record(api, st, idx);
                    let tone = if rec.tx_tone != 0 {
                        rec.tx_tone
                    } else {
                        rec.rx_tone
                    };
                    if rec.flags & FLAG_RX_ONLY == 0 {
                        (api.save_master_vfo)(rec.rx, tone);
                    } else {
                        (api.save_master_vfo)(rec.rx, rec.rx_tone);
                    }
                }
                return AppResult::Exit;
            }
        }
        KEY_UP => {
            if st.count > 0 {
                st.selected = if st.selected == 0 {
                    st.count - 1
                } else {
                    st.selected - 1
                };
            }
        }
        KEY_DOWN => {
            if st.count > 0 {
                st.selected = (st.selected + 1) % st.count;
            }
        }
        KEY_MENU => {
            if kind == KIND_SINGLE && st.count > 0 {
                let idx = st.selected;
                tune(api, st, idx);
            }
        }
        KEY_POUND => {
            if kind == KIND_SINGLE {
                st.view = match st.view {
                    View::List => View::Detail,
                    View::Detail => View::List,
                };
            }
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

fn tone_str(api: &Api, code: u16, out: &mut [u8; 18]) -> usize {
    if code == 0 {
        out[..3].copy_from_slice(b"OFF");
        return 3;
    }
    let idx = (api.subaudio_index_of_code)(code);
    if idx < 0 {
        out[..3].copy_from_slice(b"---");
        return 3;
    }
    (api.subaudio_format)(idx, out.as_mut_ptr(), out.len() as u16) as usize
}

fn draw(api: &Api) -> AppResult {
    let st = unsafe { &*core::ptr::addr_of!(STATE) };
    let snap = Snap {
        view: st.view,
        selected: st.selected,
        count: st.count,
        source_injected: matches!(st.source, Source::Injected { .. }),
        tuned: st.tuned,
    };
    if unsafe { LAST_SNAP } == Some(snap) {
        return AppResult::Continue;
    }
    unsafe {
        LAST_SNAP = Some(snap);
    }

    match st.view {
        View::List => draw_list_view(api, st),
        View::Detail => draw_detail_view(api, st),
    }
    AppResult::Continue
}

fn draw_list_view(api: &Api, st: &State) {
    static mut ROWS: [ListRow; 8] = [ListRow {
        label: [0; 16],
        value: [0; 18],
        has_value: false,
        cursor: -1,
    }; 8];

    let top = if st.selected < 8 {
        0
    } else {
        st.selected - 7
    };
    let rows = unsafe { &mut *core::ptr::addr_of_mut!(ROWS) };
    for i in 0..8 {
        let gi = top + i;
        let mut row = ListRow {
            label: [0; 16],
            value: [0; 18],
            has_value: false,
            cursor: -1,
        };
        if gi < st.count {
            let rec = record(api, st, gi);
            let nm = name_trim(&rec.name);
            row.label[..nm.len()].copy_from_slice(nm);
            let mut fbuf = [0u8; 12];
            let flen = (api.fmt_freq)(rec.rx, fbuf.as_mut_ptr()) as usize;
            let mut val = TBuf::new();
            let fstr = core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("");
            let _ = write!(val, "{}", fstr);
            if st.tuned == Some(gi) {
                let _ = write!(val, " *");
            }
            let vl = val.as_slice();
            row.value[..vl.len().min(18)].copy_from_slice(&vl[..vl.len().min(18)]);
            row.has_value = true;
        }
        rows[i] = row;
    }

    let title: &[u8] = match st.source {
        Source::Injected { local: true, .. } => b"RPTBOOK (ZIP)",
        _ => b"RPTBOOK",
    };
    (api.draw_list)(
        title.as_ptr(),
        title.len() as u16,
        rows.as_ptr(),
        8,
        top as u16,
        st.selected as u16,
        st.count as u16,
        false,
    );
}

fn draw_detail_view(api: &Api, st: &State) {
    (api.fill_rect)(0, 0, 160, 128, BLACK);
    (api.fill_rect)(0, 0, 160, 14, TITLE_BG);
    text(api, 4, 2, b"RPTBOOK DETAIL", WHITE, TITLE_BG, F6);
    if st.count == 0 {
        return;
    }
    let rec = record(api, st, st.selected);
    let nm = name_trim(&rec.name);
    text(api, 4, 18, nm, WHITE, BLACK, F9);

    let mut fbuf = [0u8; 12];
    let mut t = TBuf::new();
    let flen = (api.fmt_freq)(rec.rx, fbuf.as_mut_ptr()) as usize;
    let _ = write!(
        t,
        "RX {}",
        core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("")
    );
    text(api, 4, 42, t.as_slice(), WHITE, BLACK, F6);

    let mut t = TBuf::new();
    if rec.flags & FLAG_RX_ONLY != 0 {
        let _ = write!(t, "TX RX-ONLY");
    } else if rec.tx == 0 || rec.tx == rec.rx {
        let _ = write!(t, "TX SIMPLEX");
    } else {
        let flen = (api.fmt_freq)(rec.tx, fbuf.as_mut_ptr()) as usize;
        let _ = write!(
            t,
            "TX {}",
            core::str::from_utf8(&fbuf[..flen.min(12)]).unwrap_or("")
        );
    }
    text(api, 4, 54, t.as_slice(), WHITE, BLACK, F6);

    let mut tone = [0u8; 18];
    let n = tone_str(api, rec.tx_tone, &mut tone);
    let mut t = TBuf::new();
    let _ = write!(
        t,
        "TX TONE {}",
        core::str::from_utf8(&tone[..n]).unwrap_or("")
    );
    text(api, 4, 66, t.as_slice(), WHITE, BLACK, F6);

    let mut tone = [0u8; 18];
    let n = tone_str(api, rec.rx_tone, &mut tone);
    let mut t = TBuf::new();
    let _ = write!(
        t,
        "RX TONE {}",
        core::str::from_utf8(&tone[..n]).unwrap_or("")
    );
    text(api, 4, 78, t.as_slice(), WHITE, BLACK, F6);

    if st.tuned == Some(st.selected) {
        text(api, 4, 94, b"TUNED - MONITORING", WHITE, BLACK, F6);
    }
    text(api, 4, 114, b"MENU tune  # list", WHITE, BLACK, F6);
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
