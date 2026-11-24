//! TIMESYNC — one-shot radio clock setter for the SATELLITE app.
//!
//! The bf-5rh CPS protocol has NO clock command and the 168-byte
//! settings record has no time field, so the web flasher cannot set
//! the radio's UTC clock directly. The only programmatic setter is
//! the ABI's `utc_set` — which an overlay app can call. TIMESYNC is
//! that app: the flasher writes a timestamp into this app's flash
//! slot (the same injected-block pattern as RPTBOOK's RPBK block),
//! and opening TIMESYNC once applies it via `utc_set`.
//!
//! The firmware clock is RAM-only (`App::utc_secs`): it is lost on
//! power-off, so a fresh sync is needed after every power cycle.
//!
//! ================================================================
//! INJECTED DATA BLOCK CONTRACT (for the web flasher / page tools)
//! ================================================================
//! At sync time the page patches a small data block into this app's
//! flash slot, AFTER the .app package bytes, using the existing
//! app-install machinery (APP_WRITE at slot offset 0x6000, verified
//! by read-back; if a block is already programmed the slot is erased
//! and the app reinstalled first, exactly like RPTBOOK's flow).
//!
//! Mechanism: ABI `nor_read` can read any NOR flash address (only
//! writes are fenced to >= OVERLAY_APP_ADDR), so at startup the app
//! scans the 8 slots (base 0x14C000, stride 0x8000) for the package
//! header whose name is "TIMESYNC", then reads the block at
//! `slot_base + DATA_OFFSET` in that same slot.
//!
//!   DATA_OFFSET = 0x6000 (24,576) from the slot base.
//!   The .app package itself must stay below 0x6000 bytes.
//!
//! Block layout (all integers little-endian), mirroring RPBK:
//!   offset 0:  magic "TSYN" (4 bytes)
//!   offset 4:  u8  version = 1
//!   offset 5:  u8  reserved (0)
//!   offset 6:  u16 payload length = 8
//!   offset 8:  u16 CRC-16/XMODEM (poly 0x1021, init 0x0000) over
//!              the 8 payload bytes that follow the header
//!   offset 10: 6 bytes reserved (0)
//!   offset 16: payload (8 bytes):
//!       +0  utc_secs:     u32 — seconds since 2000-01-01T00:00:00Z
//!                         (the ABI `utc_set` argument; 0 is the
//!                         UTC_NOT_SET sentinel and is rejected)
//!       +4  written_unix: u32 — Unix seconds when the page wrote
//!                         the block (informational; shown on the
//!                         confirmation screen)
//!
//! The app validates magic, version, payload length, CRC, and
//! utc_secs != 0 before trusting the block. With no valid block it
//! shows guidance pointing at the flasher's SATELLITE panel.
//! ================================================================
//!
//! RESIDENT-SIZE RULE: this app stays on screen until a key is
//! pressed, so its segment must fit ARENA_SIZE (8,192 B total) —
//! app.x declares LENGTH = 8192 as a build-time guard (the WS-A
//! lesson: oversized segments are transient-only and fault if
//! Enter returns Continue).

#![no_std]
#![no_main]

use bf5rh_abi::{Api, AppEvent, AppResult};

const KIND_RELEASE: u8 = 1;

const BLACK: u16 = 0x0000;
const WHITE: u16 = 0xFFFF;
const GREEN: u16 = 0x07E0;
const TITLE_BG: u16 = 0x0881;
const F5: u8 = 0;
const F6: u8 = 1;
const F9: u8 = 2;

// --- injected block contract constants (see header) ---
const OVERLAY_APP_ADDR: u32 = 0x14C000;
const SLOT_SIZE: u32 = 0x8000;
const SLOT_COUNT: u32 = 8;
const DATA_OFFSET: u32 = 0x6000;
const BLOCK_HDR: usize = 16;
const PAYLOAD_LEN: u16 = 8;
const BLOCK_VERSION: u8 = 1;
/// Unix epoch (1970-01-01) to firmware epoch (2000-01-01), seconds.
const UNIX_TO_2000_SECS: u32 = 946_684_800;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pending,
    Set { secs: u32, written_unix: u32 },
    NoData,
}

static mut OUTCOME: Outcome = Outcome::Pending;
static mut DRAWN: bool = false;
static mut API_PTR: *const Api = core::ptr::null();

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

/// Find our own slot by package name, then validate an injected
/// TSYN block. Returns (utc_secs, written_unix) when valid.
fn find_block(api: &Api) -> Option<(u32, u32)> {
    let mut hdr = [0u8; 108];
    for slot in 0..SLOT_COUNT {
        let base = OVERLAY_APP_ADDR + slot * SLOT_SIZE;
        (api.nor_read)(base, hdr.as_mut_ptr(), hdr.len() as u32);
        if &hdr[0..4] != b"OVL1" {
            continue;
        }
        let name = &hdr[12..28];
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        if &name[..end] != b"TIMESYNC" {
            continue;
        }
        // our slot: validate the block at DATA_OFFSET
        let data_addr = base + DATA_OFFSET;
        let mut bh = [0u8; BLOCK_HDR];
        (api.nor_read)(data_addr, bh.as_mut_ptr(), BLOCK_HDR as u32);
        if &bh[0..4] != b"TSYN" || bh[4] != BLOCK_VERSION {
            return None;
        }
        if u16::from_le_bytes([bh[6], bh[7]]) != PAYLOAD_LEN {
            return None;
        }
        let want_crc = u16::from_le_bytes([bh[8], bh[9]]);
        let mut payload = [0u8; PAYLOAD_LEN as usize];
        (api.nor_read)(
            data_addr + BLOCK_HDR as u32,
            payload.as_mut_ptr(),
            PAYLOAD_LEN as u32,
        );
        if crc16_xmodem(&payload) != want_crc {
            return None;
        }
        let secs = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let written_unix =
            u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        if secs == 0 {
            return None; // UTC_NOT_SET sentinel — not a real time
        }
        return Some((secs, written_unix));
    }
    None
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's
/// algorithm). Returns (year, month 1..=12, day 1..=31).
fn civil_from_days(z_in: i64) -> (i64, u32, u32) {
    let z = z_in + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Split seconds-since-2000 into (year, month, day, hh, mm, ss).
fn split_datetime(secs: u32) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64 + 10_957; // 2000-01-01 in days since 1970
    let rem = secs % 86_400;
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn put2(out: &mut [u8], i: &mut usize, v: u32) {
    out[*i] = b'0' + ((v / 10) % 10) as u8;
    out[*i + 1] = b'0' + (v % 10) as u8;
    *i += 2;
}

fn put4(out: &mut [u8], i: &mut usize, v: i64) {
    let v = if v < 0 { 0 } else { v as u32 };
    out[*i] = b'0' + ((v / 1000) % 10) as u8;
    out[*i + 1] = b'0' + ((v / 100) % 10) as u8;
    out[*i + 2] = b'0' + ((v / 10) % 10) as u8;
    out[*i + 3] = b'0' + (v % 10) as u8;
    *i += 4;
}

/// "YYYY-MM-DD" (10 bytes) from seconds-since-2000.
fn fmt_date(secs: u32, out: &mut [u8]) {
    let (y, mo, d, _, _, _) = split_datetime(secs);
    let mut i = 0;
    put4(out, &mut i, y);
    out[i] = b'-';
    i += 1;
    put2(out, &mut i, mo);
    out[i] = b'-';
    i += 1;
    put2(out, &mut i, d);
}

/// "HH:MM:SS" (8 bytes) from seconds-since-2000.
fn fmt_time(secs: u32, out: &mut [u8]) {
    let (_, _, _, hh, mm, ss) = split_datetime(secs);
    let mut i = 0;
    put2(out, &mut i, hh);
    out[i] = b':';
    i += 1;
    put2(out, &mut i, mm);
    out[i] = b':';
    i += 1;
    put2(out, &mut i, ss);
}

fn text(api: &Api, x: i16, y: i16, s: &[u8], fg: u16, bg: u16, font: u8) {
    (api.draw_text)(x, y, s.as_ptr(), s.len() as u16, fg, bg, font);
}

#[no_mangle]
pub extern "C" fn app_entry(api: &Api, ev: AppEvent) -> AppResult {
    unsafe {
        API_PTR = api as *const Api;
    }
    match ev {
        AppEvent::Enter => enter(api),
        AppEvent::Key { id: _, kind } => {
            // Any key press exits (a release alone does not).
            if kind != KIND_RELEASE {
                AppResult::Exit
            } else {
                AppResult::Continue
            }
        }
        AppEvent::Tick { dt_100us: _ } => AppResult::Continue,
        AppEvent::Draw => draw(api),
        AppEvent::Leave => AppResult::Continue,
    }
}

fn enter(api: &Api) -> AppResult {
    let outcome = match find_block(api) {
        Some((secs, written_unix)) => {
            (api.utc_set)(secs);
            Outcome::Set { secs, written_unix }
        }
        None => Outcome::NoData,
    };
    unsafe {
        OUTCOME = outcome;
        DRAWN = false;
    }
    AppResult::Continue
}

fn draw(api: &Api) -> AppResult {
    if unsafe { DRAWN } {
        return AppResult::Continue;
    }
    unsafe {
        DRAWN = true;
    }
    let outcome = unsafe { OUTCOME };

    (api.fill_rect)(0, 0, 160, 128, BLACK);
    (api.fill_rect)(0, 0, 160, 14, TITLE_BG);
    text(api, 4, 2, b"TIMESYNC", WHITE, TITLE_BG, F6);

    match outcome {
        Outcome::Set { secs, written_unix } => {
            text(api, 4, 20, b"CLOCK SET", GREEN, BLACK, F9);
            let mut date = [0u8; 10];
            fmt_date(secs, &mut date);
            text(api, 4, 44, &date, WHITE, BLACK, F9);
            let mut line = [0u8; 12];
            fmt_time(secs, &mut line[0..8]);
            line[8] = b' ';
            line[9] = b'U';
            line[10] = b'T';
            line[11] = b'C';
            text(api, 4, 68, &line, WHITE, BLACK, F9);
            if written_unix != 0 && written_unix > UNIX_TO_2000_SECS {
                // "WRITTEN YYYY-MM-DD HH:MM" in the small font
                let wsecs = written_unix - UNIX_TO_2000_SECS;
                let mut wl = [0u8; 24];
                wl[0..8].copy_from_slice(b"WRITTEN ");
                fmt_date(wsecs, &mut wl[8..18]);
                wl[18] = b' ';
                let (_, _, _, hh, mm, _) = split_datetime(wsecs);
                let mut i = 19;
                put2(&mut wl, &mut i, hh);
                wl[i] = b':';
                i += 1;
                put2(&mut wl, &mut i, mm);
                text(api, 4, 94, &wl[..i], WHITE, BLACK, F5);
            }
            text(api, 4, 114, b"ANY KEY = EXIT", WHITE, BLACK, F6);
        }
        _ => {
            text(api, 4, 20, b"NO SYNC DATA", WHITE, BLACK, F9);
            text(api, 4, 48, b"No sync data - use", WHITE, BLACK, F6);
            text(api, 4, 62, b"the web flasher:", WHITE, BLACK, F6);
            text(api, 4, 76, b"SATELLITE panel ->", WHITE, BLACK, F6);
            text(api, 4, 90, b"Sync radio clock", WHITE, BLACK, F6);
            text(api, 4, 114, b"ANY KEY = EXIT", WHITE, BLACK, F6);
        }
    }
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
