//! Scenario files and the run loop.
//!
//! A scenario is a line-based script (`#` comments):
//! ```text
//! utc 816912000          # wall clock, seconds since 2000-01-01T00:00Z
//! assertutc 816912000    # fail the run unless harness UTC ≈ this (±2 s)
//! wait 500               # advance virtual time 500 ms (10 ms ticks)
//! key DOWN               # key event, kind defaults to "single"
//! key EXIT long
//! tx 462562500 -80 1500  # RF burst starting now: freq_hz dBm dur_ms
//! shot list              # save frame: list.png + list@4x.png
//! note some text         # log line
//! ```
//! Key names: 0..9, STAR, POUND, MENU, EXIT, UP, DOWN, VM, AB, BAND,
//! SIDE1, SIDE2. Kinds: press, release, single, long, repeat.
//!
//! The run loop mirrors the firmware's dispatch (src/app/overlay.rs):
//! Enter once (following Chain results, max 4 hops), then for each step:
//! keys are dispatched with no LCD, waits are Tick events, and a Draw
//! (with full-screen clear after Enter/Chain) follows state changes.
//!
//! It also mirrors the firmware loader's size rules, using the real
//! segment sizes gen_wrappers.py reads from the built .app packages:
//! a segment over ARENA_MAX is refused (TooBig), and an oversized
//! segment (over ARENA_SIZE) must chain straight out of Enter —
//! returning Continue faults FAULT_OVERSIZE_CONTINUE, and any later
//! event faults FAULT_OVERSIZE_RESIDENT. Interactive apps therefore
//! have to fit ARENA_SIZE; oversized segments are transient compute
//! only (the satellite predict pattern).

use std::path::PathBuf;

use bf5rh_abi::{AppEntryFn, AppEvent, AppResult};

use crate::api;
use crate::rf::RfModel;
use crate::state::{self, SimState};

pub struct RunConfig {
    pub scenario: PathBuf,
    pub out_dir: PathBuf,
    pub data_dir: PathBuf,
    pub rf_file: Option<PathBuf>,
    /// (slot, file) app packages to place in mock flash.
    pub slot_apps: Vec<(u8, PathBuf)>,
    /// (slot, offset, file) raw blobs to patch into a slot.
    pub slot_data: Vec<(u8, u32, PathBuf)>,
    /// Whole flash image to preload.
    pub flash_file: Option<PathBuf>,
    /// Where to save the flash image at the end (persistence checks).
    pub flash_out: Option<PathBuf>,
}

const TICK_100US: u32 = 100; // 10 ms ticks

fn key_id(name: &str) -> Option<u8> {
    Some(match name.to_ascii_uppercase().as_str() {
        "0" => 0,
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        "6" => 6,
        "7" => 7,
        "8" => 8,
        "9" => 9,
        "STAR" | "*" => 10,
        "POUND" | "#" => 11,
        "MENU" => 12,
        "EXIT" => 13,
        "UP" => 14,
        "DOWN" => 15,
        "VM" => 16,
        "AB" => 17,
        "BAND" => 18,
        "SIDE1" => 19,
        "SIDE2" => 20,
        _ => return None,
    })
}

fn kind_id(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "press" => 0,
        "release" => 1,
        "single" => 2,
        "long" => 3,
        "repeat" => 4,
        _ => return None,
    })
}

struct Runner {
    api: bf5rh_abi::Api,
    segments: Vec<AppEntryFn>,
    /// Real (image_len + bss_len) per segment from the built .app
    /// package; empty when unknown (loader oversize rules then skipped).
    seg_sizes: Vec<u32>,
    current: usize,
    out_dir: PathBuf,
    exited: bool,
    last_was_enter: bool,
}

impl Runner {
    fn seg_size(&self) -> u32 {
        self.seg_sizes.get(self.current).copied().unwrap_or(0)
    }

    fn seg_oversized(&self) -> bool {
        self.seg_size() as usize > bf5rh_abi::ARENA_SIZE
    }

    /// Firmware `load_segment` size gate: a segment above ARENA_MAX is
    /// refused outright (LoadError::TooBig).
    fn load_refused(&self) -> bool {
        let sz = self.seg_size();
        if sz as usize > bf5rh_abi::ARENA_MAX {
            state::with(|s| {
                s.log_event(format!(
                    "[loader] segment {} is {sz} B > ARENA_MAX {} — firmware refuses the load (TooBig); app never starts",
                    self.current,
                    bf5rh_abi::ARENA_MAX
                ));
                s.fatal = Some("loader TooBig".into());
            });
            return true;
        }
        false
    }

    fn call(&mut self, ev: AppEvent) -> AppResult {
        self.last_was_enter = matches!(ev, AppEvent::Enter);
        // Firmware backstop (`overlay.rs call`): while an oversized
        // segment is resident, any non-Enter event faults without the
        // app ever seeing it (FAULT_OVERSIZE_RESIDENT).
        if !self.last_was_enter && self.seg_oversized() {
            state::with(|s| {
                s.log_event(format!(
                    "[loader] non-Enter event while oversized segment {} ({} B) is resident — firmware faults FAULT_OVERSIZE_RESIDENT (0xE0000001)",
                    self.current,
                    self.seg_size()
                ));
                s.fatal = Some("Fault(0xE0000001) oversize-resident".into());
            });
            self.exited = true;
            return AppResult::Fault(0xE000_0001);
        }
        let entry = self.segments[self.current];
        entry(&self.api, ev)
    }

    /// Firmware `handle_result`: follow Chain into the next segment
    /// (Enter it, reset the list cache, force a full clear), stop on
    /// Exit/Fault.
    fn handle(&mut self, mut result: AppResult) {
        let mut hops = 0u8;
        loop {
            // Firmware `enter_call`: an oversized segment aliases the
            // bottom of the stack, so it must chain straight out of
            // Enter. Returning Continue faults with
            // FAULT_OVERSIZE_CONTINUE and the app is left immediately —
            // on hardware such an app appears to never launch.
            if self.last_was_enter
                && matches!(result, AppResult::Continue)
                && self.seg_oversized()
            {
                state::with(|s| {
                    s.log_event(format!(
                        "[loader] segment {} is {} B (> ARENA_SIZE {}): Enter returned Continue — firmware faults FAULT_OVERSIZE_CONTINUE (0xE0000002) and leaves the app; on hardware it never launches",
                        self.current,
                        self.seg_size(),
                        bf5rh_abi::ARENA_SIZE
                    ));
                    s.fatal = Some("Fault(0xE0000002) oversize-continue".into());
                });
                self.exited = true;
                return;
            }
            match result {
                AppResult::Continue => return,
                AppResult::Exit => {
                    state::with(|s| s.log_event("app returned Exit".into()));
                    self.exited = true;
                    return;
                }
                AppResult::Fault(line) => {
                    state::with(|s| {
                        s.log_event(format!("app returned Fault({line})"));
                        s.fatal = Some(format!("Fault({line})"));
                    });
                    self.exited = true;
                    return;
                }
                AppResult::Chain(seg) => {
                    hops += 1;
                    if hops > 4 || seg as usize >= self.segments.len() {
                        state::with(|s| {
                            s.log_event(format!(
                                "Chain({seg}) refused (hop {hops}, {} segments)",
                                self.segments.len()
                            ));
                            s.fatal = Some(format!("bad Chain({seg})"));
                        });
                        self.exited = true;
                        return;
                    }
                    self.current = seg as usize;
                    state::with(|s| {
                        s.list_cache = None;
                        s.full_clear_pending = true;
                        s.tx_enabled = false;
                        s.holds_rx = false;
                        s.backlight_hold = false;
                        // (handoff buffer deliberately kept: it survives Chain)
                    });
                    if self.load_refused() {
                        self.exited = true;
                        return;
                    }
                    result = self.call(AppEvent::Enter);
                }
            }
        }
    }

    fn draw(&mut self) {
        if self.exited {
            return;
        }
        state::with(|s| {
            if s.full_clear_pending {
                s.fb.clear(0x0000);
                s.full_clear_pending = false;
            }
            s.drawing = true;
        });
        let r = self.call(AppEvent::Draw);
        state::with(|s| s.drawing = false);
        self.handle(r);
    }

    fn shot(&mut self, name: &str) {
        self.draw();
        let base = self.out_dir.join(name);
        state::with(|s| s.fb.save_pngs(&base));
    }

    fn tick_wait(&mut self, ms: u64) {
        let mut remaining = ms * 10; // in 100us units
        while remaining > 0 && !self.exited {
            let dt = remaining.min(TICK_100US as u64) as u32;
            state::with(|s| s.clock_100us += dt as u64);
            let r = self.call(AppEvent::Tick { dt_100us: dt });
            self.handle(r);
            remaining -= dt as u64;
        }
    }
}

pub fn run(segments: &[AppEntryFn], seg_sizes: &[u32], cfg: RunConfig) -> i32 {
    // ---- build initial state ----
    let mut st = SimState::new();
    if let Some(rf_path) = &cfg.rf_file {
        match RfModel::load(rf_path) {
            Ok(m) => st.rf = m,
            Err(e) => {
                eprintln!("[sim] RF config error: {e}");
                return 2;
            }
        }
    }
    // Preston's real data: settings record + channel table.
    let settings_path = cfg.data_dir.join("settings.bin");
    if let Ok(bytes) = std::fs::read(&settings_path) {
        if bytes.len() >= crate::state::SETTINGS_LEN {
            st.settings
                .copy_from_slice(&bytes[..crate::state::SETTINGS_LEN]);
            st.sql_level = st.settings[0];
            println!("[sim] settings loaded from {}", settings_path.display());
        }
    }
    let channels_path = cfg.data_dir.join("channels.bin");
    if let Ok(bytes) = std::fs::read(&channels_path) {
        let n = (bytes.len() / 32).min(crate::state::MAX_CHANNELS);
        for i in 0..n {
            st.channels[i].copy_from_slice(&bytes[i * 32..i * 32 + 32]);
        }
        // Master VFO starts on the first programmed channel, like a radio
        // that was left in channel mode.
        for rec in st.channels.iter() {
            if rec.iter().any(|&b| b != 0xFF) {
                let deci = bcd4(&rec[0..4]);
                if deci > 0 {
                    st.master_freq_hz = deci * 10;
                    st.rx_freq_hz = deci * 10;
                    st.tx_freq_hz = deci * 10;
                }
                break;
            }
        }
        println!("[sim] channels loaded from {}", channels_path.display());
    }
    // Flash preloads.
    if let Some(flash_path) = &cfg.flash_file {
        match std::fs::read(flash_path) {
            Ok(bytes) => {
                let n = bytes.len().min(st.flash.len());
                st.flash[..n].copy_from_slice(&bytes[..n]);
                println!("[sim] flash image loaded from {}", flash_path.display());
            }
            Err(e) => {
                eprintln!("[sim] cannot read flash image {}: {e}", flash_path.display());
                return 2;
            }
        }
    }
    for (slot, file) in &cfg.slot_apps {
        match std::fs::read(file) {
            Ok(bytes) => {
                let base = crate::state::OVERLAY_APP_ADDR as usize + *slot as usize * 0x8000;
                let n = bytes.len().min(0x8000);
                st.flash[base..base + n].copy_from_slice(&bytes[..n]);
                println!("[sim] app package {} -> slot {slot}", file.display());
            }
            Err(e) => {
                eprintln!("[sim] cannot read app package {}: {e}", file.display());
                return 2;
            }
        }
    }
    for (slot, offset, file) in &cfg.slot_data {
        match std::fs::read(file) {
            Ok(bytes) => {
                let base = crate::state::OVERLAY_APP_ADDR as usize
                    + *slot as usize * 0x8000
                    + *offset as usize;
                if base + bytes.len() > st.flash.len() {
                    eprintln!("[sim] slot-data {} overflows mock flash", file.display());
                    return 2;
                }
                st.flash[base..base + bytes.len()].copy_from_slice(&bytes);
                println!(
                    "[sim] slot data {} -> slot {slot} + 0x{offset:X}",
                    file.display()
                );
            }
            Err(e) => {
                eprintln!("[sim] cannot read slot data {}: {e}", file.display());
                return 2;
            }
        }
    }
    // Default wall clock: 2026-11-24T08:00:00Z in seconds since 2000.
    st.set_utc(816_940_800);
    state::init(st);

    // ---- parse scenario ----
    let text = match std::fs::read_to_string(&cfg.scenario) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[sim] cannot read scenario {}: {e}", cfg.scenario.display());
            return 2;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&cfg.out_dir) {
        eprintln!("[sim] cannot create {}: {e}", cfg.out_dir.display());
        return 2;
    }

    let mut runner = Runner {
        api: api::make_api(),
        segments: segments.to_vec(),
        seg_sizes: seg_sizes.to_vec(),
        current: 0,
        out_dir: cfg.out_dir.clone(),
        exited: false,
        last_was_enter: false,
    };

    // Enter segment 0 (firmware: load, clear-on-draw, reset per-app flags).
    if runner.load_refused() {
        return 1;
    }
    let r = runner.call(AppEvent::Enter);
    runner.handle(r);
    runner.shot("enter");

    for (ln, raw) in text.lines().enumerate() {
        if runner.exited {
            break;
        }
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap();
        let rest = parts.next().unwrap_or("").trim();
        let bad = |what: &str| -> i32 {
            eprintln!("[sim] scenario {}:{}: {what}", cfg.scenario.display(), ln + 1);
            2
        };
        match cmd {
            "utc" => {
                let v: u32 = match rest.parse() {
                    Ok(v) => v,
                    Err(_) => return bad("utc needs seconds since 2000-01-01"),
                };
                state::with(|s| s.set_utc(v));
            }
            "assertutc" => {
                let v: u32 = match rest.parse() {
                    Ok(v) => v,
                    Err(_) => return bad("assertutc needs seconds since 2000-01-01"),
                };
                let now = state::with(|s| s.utc_now());
                let diff = now.abs_diff(v);
                if diff > 2 {
                    return bad(&format!(
                        "assertutc failed: harness UTC is {now}, expected {v} (±2 s)"
                    ));
                }
                println!("[scenario] assertutc ok: harness UTC = {now} (expected {v})");
            }
            "wait" => {
                let ms: u64 = match rest.parse() {
                    Ok(v) => v,
                    Err(_) => return bad("wait needs milliseconds"),
                };
                runner.tick_wait(ms);
                runner.draw();
            }
            "key" => {
                let f: Vec<&str> = rest.split_whitespace().collect();
                if f.is_empty() {
                    return bad("key needs a name");
                }
                let Some(id) = key_id(f[0]) else {
                    return bad("unknown key name");
                };
                let kind = if f.len() > 1 {
                    match kind_id(f[1]) {
                        Some(k) => k,
                        None => return bad("unknown key kind"),
                    }
                } else {
                    2 // single
                };
                let r = runner.call(AppEvent::Key { id, kind });
                runner.handle(r);
                runner.draw();
            }
            "tx" => {
                let f: Vec<&str> = rest.split_whitespace().collect();
                if f.len() != 3 {
                    return bad("tx needs: freq_hz dBm dur_ms");
                }
                let (Ok(freq), Ok(dbm), Ok(dur)) = (
                    f[0].parse::<u32>(),
                    f[1].parse::<i32>(),
                    f[2].parse::<u64>(),
                ) else {
                    return bad("tx arguments must be numbers");
                };
                state::with(|s| {
                    let start = s.clock_100us;
                    s.rf.bursts.push(crate::rf::Burst {
                        freq_hz: freq,
                        start_100us: start,
                        dur_100us: dur * 10,
                        dbm,
                    });
                    s.log_event(format!("tx burst: {freq} Hz at {dbm} dBm for {dur} ms"));
                });
            }
            "shot" => {
                if rest.is_empty() {
                    return bad("shot needs a name");
                }
                runner.shot(rest);
            }
            "note" => {
                println!("[scenario] {rest}");
            }
            _ => return bad("unknown command"),
        }
    }

    if !runner.exited {
        let r = runner.call(AppEvent::Leave);
        runner.handle(r);
    }

    if let Some(out) = &cfg.flash_out {
        state::with(|s| {
            if let Err(e) = std::fs::write(out, &s.flash) {
                eprintln!("[sim] cannot write flash image {}: {e}", out.display());
            } else {
                println!("[sim] flash image saved to {}", out.display());
            }
        });
    }

    // ---- report ----
    let (fatal, called) = state::with(|s| (s.fatal.clone(), s.called.clone()));
    println!("[sim] API coverage this run: {} callbacks used", called.len());
    println!("[sim]   {}", called.iter().copied().collect::<Vec<_>>().join(", "));
    if let Some(f) = fatal {
        eprintln!("[sim] run ended with fault: {f}");
        return 3;
    }
    0
}

fn bcd4(b: &[u8]) -> u32 {
    let dec = |x: u8| ((x >> 4) * 10 + (x & 0x0F)) as u32;
    dec(b[3]) * 1_000_000 + dec(b[2]) * 10_000 + dec(b[1]) * 100 + dec(b[0])
}

pub fn parse_args(app_name: &str, args: &[String]) -> Result<RunConfig, String> {
    let mut cfg = RunConfig {
        scenario: PathBuf::new(),
        out_dir: PathBuf::from(format!("runs/{app_name}")),
        data_dir: PathBuf::from("data"),
        rf_file: None,
        slot_apps: Vec::new(),
        slot_data: Vec::new(),
        flash_file: None,
        flash_out: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => cfg.out_dir = PathBuf::from(it.next().ok_or("--out needs a dir")?),
            "--data" => cfg.data_dir = PathBuf::from(it.next().ok_or("--data needs a dir")?),
            "--rf" => cfg.rf_file = Some(PathBuf::from(it.next().ok_or("--rf needs a file")?)),
            "--flash" => {
                cfg.flash_file = Some(PathBuf::from(it.next().ok_or("--flash needs a file")?))
            }
            "--flash-out" => {
                cfg.flash_out = Some(PathBuf::from(it.next().ok_or("--flash-out needs a file")?))
            }
            "--slot-app" => {
                let v = it.next().ok_or("--slot-app needs SLOT:FILE")?;
                let (slot, file) = v.split_once(':').ok_or("--slot-app needs SLOT:FILE")?;
                cfg.slot_apps.push((
                    slot.parse().map_err(|_| "bad slot number")?,
                    PathBuf::from(file),
                ));
            }
            "--slot-data" => {
                let v = it.next().ok_or("--slot-data needs SLOT:OFFSET:FILE")?;
                let mut it2 = v.splitn(3, ':');
                let slot: u8 = it2
                    .next()
                    .and_then(|x| x.parse().ok())
                    .ok_or("bad slot in --slot-data")?;
                let off_s = it2.next().ok_or("bad offset in --slot-data")?;
                let offset = u32::from_str_radix(off_s.trim_start_matches("0x"), 16)
                    .map_err(|_| "offset must be hex")?;
                let file = it2.next().ok_or("missing file in --slot-data")?;
                cfg.slot_data.push((slot, offset, PathBuf::from(file)));
            }
            other if !other.starts_with("--") && cfg.scenario.as_os_str().is_empty() => {
                cfg.scenario = PathBuf::from(other);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if cfg.scenario.as_os_str().is_empty() {
        return Err(format!(
            "usage: sim-{app_name} <scenario.txt> [--out DIR] [--data DIR] [--rf FILE] [--flash FILE] [--flash-out FILE] [--slot-app SLOT:FILE] [--slot-data SLOT:0xOFFSET:FILE]"
        ));
    }
    Ok(cfg)
}

pub fn main_for(app_name: &str, segments: &[AppEntryFn], seg_sizes: &[u32]) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(app_name, &args) {
        Ok(cfg) => std::process::exit(run(segments, seg_sizes, cfg)),
        Err(e) => {
            eprintln!("[sim] {e}");
            std::process::exit(2);
        }
    }
}
