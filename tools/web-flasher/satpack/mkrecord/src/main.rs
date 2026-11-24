//! mkrecord — pack a satellite list (.sat) into the 5,120-byte on-flash
//! satellite table the SATELLITE overlay app reads at flash 0x18C000.
//!
//! This is the host packer upstream's `tools/push_sat.py` expects at
//! `tools/mkrecord/` (it was missing from the tree; this is a clean-room
//! implementation against the `sgp4mini` crate's own `Record::encode`, so
//! encode/decode can never drift apart).
//!
//! Usage: mkrecord <input.sat> <output.bin>
//!
//! .sat format (line-based; blank lines and `#` comments ignored):
//!
//! ```text
//! sat <name> <rx_hz> <tx_hz> <rx_tone> <tx_tone>
//! tle1 <TLE line 1, 69 chars>
//! tle2 <TLE line 2, 69 chars>
//! ```
//!
//! Repeat per satellite, max 20. Tones use the firmware subaudio encoding
//! (0 = none, 1-105 DCS normal, 106-250 DCS inverted, >250 = CTCSS in
//! tenths of a Hz, e.g. 670 = 67.0 Hz). tx_hz 0 = receive-only.
//! Unused table slots are filled with 0xFF (the app's erased-slot marker).

use std::process::ExitCode;

use sgp4mini::record::Record;
use sgp4mini::{Constants, Elements};
use sgp4mini::{days_since_j2000_from_tle_epoch, MAX_SATELLITES, TABLE_BYTES};

struct SatIn {
    name: String,
    rx_hz: u32,
    tx_hz: u32,
    rx_tone: u16,
    tx_tone: u16,
    tle1: String,
    tle2: String,
}

fn parse_f64(field: &str, what: &str) -> Result<f64, String> {
    field
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("bad {what}: {field:?}"))
}

/// TLE B* drag term, columns 54-61: [sign/space][5 digits][exp sign][exp digit]
/// meaning sign * 0.digits * 10^(+-exp).
fn parse_bstar(field: &str) -> Result<f64, String> {
    let b = field.as_bytes();
    if b.len() < 8 {
        return Err(format!("B* field too short: {field:?}"));
    }
    let sign = if b[0] == b'-' { -1.0 } else { 1.0 };
    let digits = std::str::from_utf8(&b[1..6]).map_err(|_| "B* digits not utf8")?;
    let mantissa: f64 = format!("0.{digits}")
        .parse()
        .map_err(|_| format!("bad B* mantissa: {digits:?}"))?;
    let exp_sign = if b[6] == b'-' { -1.0f64 } else { 1.0 };
    let exp_digit = (b[7] as char)
        .to_digit(10)
        .ok_or_else(|| format!("bad B* exponent in {field:?}"))? as f64;
    Ok(sign * mantissa * 10f64.powf(exp_sign * exp_digit))
}

fn elements_from_tle(tle1: &str, tle2: &str) -> Result<Elements, String> {
    if tle1.len() < 61 || tle2.len() < 63 {
        return Err("TLE line too short".into());
    }
    let epoch_year: i32 = tle1[18..20]
        .trim()
        .parse()
        .map_err(|_| "bad TLE epoch year")?;
    let epoch_day = parse_f64(&tle1[20..32], "TLE epoch day")?;
    let drag_term = parse_bstar(&tle1[53..61])?;

    let deg = std::f64::consts::PI / 180.0;
    let inclination = parse_f64(&tle2[8..16], "inclination")? * deg;
    let right_ascension = parse_f64(&tle2[17..25], "RAAN")? * deg;
    let ecc_digits = tle2[26..33].trim();
    let eccentricity: f64 = format!("0.{ecc_digits}")
        .parse()
        .map_err(|_| format!("bad eccentricity: {ecc_digits:?}"))?;
    let argument_of_perigee = parse_f64(&tle2[34..42], "argument of perigee")? * deg;
    let mean_anomaly = parse_f64(&tle2[43..51], "mean anomaly")? * deg;
    let mean_motion_rev_day = parse_f64(&tle2[52..63], "mean motion")?;
    // Kozai convention, rad/min.
    let mean_motion = mean_motion_rev_day * 2.0 * std::f64::consts::PI / 1440.0;

    Ok(Elements {
        epoch_days_j2000: days_since_j2000_from_tle_epoch(epoch_year, epoch_day),
        inclination,
        right_ascension,
        eccentricity,
        argument_of_perigee,
        mean_anomaly,
        mean_motion,
        drag_term,
    })
}

fn parse_sat_file(text: &str) -> Result<Vec<SatIn>, String> {
    let mut sats: Vec<SatIn> = Vec::new();
    let mut pending: Option<SatIn> = None;
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let kw = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match kw {
            "sat" => {
                if let Some(p) = pending.take() {
                    sats.push(p);
                }
                let f: Vec<&str> = rest.split_whitespace().collect();
                if f.len() != 5 {
                    return Err(format!(
                        "line {}: 'sat' needs exactly: name rx_hz tx_hz rx_tone tx_tone \
                         (name must not contain spaces)",
                        lineno + 1
                    ));
                }
                // Name is the first field; names may not contain spaces
                // (the on-flash name field is 10 bytes, NUL-padded).
                let name = f[0].to_string();
                let num = |i: usize, what: &str| -> Result<u32, String> {
                    f[i].parse::<u32>()
                        .map_err(|_| format!("line {}: bad {what}: {:?}", lineno + 1, f[i]))
                };
                pending = Some(SatIn {
                    name,
                    rx_hz: num(1, "rx_hz")?,
                    tx_hz: num(2, "tx_hz")?,
                    rx_tone: num(3, "rx_tone")? as u16,
                    tx_tone: num(4, "tx_tone")? as u16,
                    tle1: String::new(),
                    tle2: String::new(),
                });
            }
            "tle1" | "tle2" => {
                let p = pending
                    .as_mut()
                    .ok_or_else(|| format!("line {}: {kw} before any 'sat'", lineno + 1))?;
                if kw == "tle1" {
                    p.tle1 = rest.to_string();
                } else {
                    p.tle2 = rest.to_string();
                }
            }
            other => {
                return Err(format!("line {}: unknown keyword {other:?}", lineno + 1));
            }
        }
    }
    if let Some(p) = pending.take() {
        sats.push(p);
    }
    Ok(sats)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err(format!(
            "usage: {} <input.sat> <output.bin>",
            args.first().map(|s| s.as_str()).unwrap_or("mkrecord")
        ));
    }
    let text = std::fs::read_to_string(&args[1])
        .map_err(|e| format!("cannot read {}: {e}", args[1]))?;
    let sats = parse_sat_file(&text)?;
    if sats.is_empty() {
        return Err("no satellites in input".into());
    }
    if sats.len() > MAX_SATELLITES {
        return Err(format!(
            "{} satellites, table holds {MAX_SATELLITES}",
            sats.len()
        ));
    }

    let mut table = vec![0xFFu8; TABLE_BYTES];
    for (i, sat) in sats.iter().enumerate() {
        if sat.tle1.is_empty() || sat.tle2.is_empty() {
            return Err(format!("{}: missing TLE lines", sat.name));
        }
        if !sat.name.is_ascii() || sat.name.len() > 10 {
            return Err(format!(
                "{}: name must be ASCII, <= 10 chars",
                sat.name
            ));
        }
        let elements = elements_from_tle(&sat.tle1, &sat.tle2)
            .map_err(|e| format!("{}: {e}", sat.name))?;
        let constants = Constants::new(&elements)
            .map_err(|e| format!("{}: Constants::new failed: {e:?}", sat.name))?;
        let mut name = [0u8; 10];
        name[..sat.name.len()].copy_from_slice(sat.name.as_bytes());
        let record = Record {
            name,
            rx_freq_hz: sat.rx_hz,
            tx_freq_hz: sat.tx_hz,
            rx_tone: sat.rx_tone,
            tx_tone: sat.tx_tone,
            epoch_days_j2000: elements.epoch_days_j2000,
            constants,
        };
        record.encode(&mut table[i * 256..(i + 1) * 256]);
        eprintln!(
            "packed {:<10} epoch {:.2} d (J2000) rx {} tx {}",
            sat.name, elements.epoch_days_j2000, sat.rx_hz, sat.tx_hz
        );
    }

    std::fs::write(&args[2], &table).map_err(|e| format!("cannot write {}: {e}", args[2]))?;
    eprintln!("wrote {} ({} bytes, {} satellites)", args[2], table.len(), sats.len());
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mkrecord: {e}");
            ExitCode::FAILURE
        }
    }
}
