//! The simulated RF environment: a static per-frequency signal level
//! table, scenario-scheduled transmission bursts, and an FM broadcast
//! station list. Deliberately simple — it exists so apps that scan,
//! measure RSSI, or detect squelch behave deterministically.
//!
//! Config file format (line-based, `#` comments):
//! ```text
//! noise_dbm -135          # level everywhere unlisted
//! sql_open_dbm -117       # squelch threshold before sql-level adjustment
//! rf 162475000 -42        # static level: freq_hz dBm [tone_index]
//! rf 462562500 -85 22     #   optional 3rd field: subaudio index for detect_subaudio
//! fm 96100 200            # FM station: freq_khz RSSI(0..255)
//! ```
//! A built-in default (Preston's Lowell/Gastonia environment) is used
//! when no config file is given: NOAA channels strong with 162.475 MHz
//! at -42 dBm (his bench measurement), a couple of GMRS repeaters
//! moderate, a few Charlotte FM stations.

#[derive(Clone, Copy, Debug)]
pub struct Burst {
    pub freq_hz: u32,
    pub start_100us: u64,
    pub dur_100us: u64,
    pub dbm: i32,
}

#[derive(Clone, Debug)]
pub struct RfModel {
    pub noise_dbm: i32,
    pub sql_open_dbm: i32,
    /// (freq_hz, dbm, Option<subaudio index>)
    pub levels: Vec<(u32, i32, Option<i32>)>,
    pub bursts: Vec<Burst>,
    /// (freq_khz, rssi 0..255)
    pub fm_stations: Vec<(u32, u8)>,
}

impl Default for RfModel {
    fn default() -> Self {
        let mut m = RfModel {
            noise_dbm: -135,
            sql_open_dbm: -117,
            levels: Vec::new(),
            bursts: Vec::new(),
            fm_stations: Vec::new(),
        };
        // NOAA weather channels (Hz, dBm). 162.475 = Preston's strongest.
        for (hz, dbm) in [
            (162_400_000, -78),
            (162_425_000, -96),
            (162_450_000, -101),
            (162_475_000, -42),
            (162_500_000, -90),
            (162_525_000, -108),
            (162_550_000, -112),
        ] {
            m.levels.push((hz, dbm, None));
        }
        // A couple of GMRS repeater outputs at moderate strength.
        m.levels.push((462_562_500, -88, None));
        m.levels.push((462_625_000, -81, None));
        m.levels.push((462_725_000, -93, None));
        // Ham calling frequencies: quiet but present.
        m.levels.push((146_520_000, -120, None));
        m.levels.push((446_000_000, -122, None));
        // FM broadcast stations (kHz, RSSI 0..255), Charlotte-ish dial.
        for (khz, rssi) in [
            (88_500, 90),
            (90_700, 130),
            (96_100, 210),
            (99_700, 165),
            (101_900, 120),
            (104_700, 235),
            (107_900, 180),
        ] {
            m.fm_stations.push((khz, rssi));
        }
        m
    }
}

impl RfModel {
    pub fn load(path: &std::path::Path) -> Result<RfModel, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let mut m = RfModel {
            noise_dbm: -135,
            sql_open_dbm: -117,
            levels: Vec::new(),
            bursts: Vec::new(),
            fm_stations: Vec::new(),
        };
        for (ln, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            let err = |what: &str| format!("{}:{}: bad {what}", path.display(), ln + 1);
            match f[0] {
                "noise_dbm" if f.len() == 2 => {
                    m.noise_dbm = f[1].parse().map_err(|_| err("noise_dbm"))?
                }
                "sql_open_dbm" if f.len() == 2 => {
                    m.sql_open_dbm = f[1].parse().map_err(|_| err("sql_open_dbm"))?
                }
                "rf" if f.len() == 3 || f.len() == 4 => {
                    let hz: u32 = f[1].parse().map_err(|_| err("rf freq"))?;
                    let dbm: i32 = f[2].parse().map_err(|_| err("rf dbm"))?;
                    let tone = if f.len() == 4 {
                        Some(f[3].parse().map_err(|_| err("rf tone"))?)
                    } else {
                        None
                    };
                    m.levels.push((hz, dbm, tone));
                }
                "fm" if f.len() == 3 => {
                    let khz: u32 = f[1].parse().map_err(|_| err("fm freq"))?;
                    let rssi: u8 = f[2].parse().map_err(|_| err("fm rssi"))?;
                    m.fm_stations.push((khz, rssi));
                }
                _ => return Err(err("line")),
            }
        }
        Ok(m)
    }

    /// Signal level at `freq_hz` at virtual time `t_100us`: the strongest
    /// of the static table (within 5 kHz) and any active burst.
    pub fn dbm_at(&self, freq_hz: u32, t_100us: u64) -> i32 {
        let mut dbm = self.noise_dbm;
        for &(hz, level, _) in &self.levels {
            if hz.abs_diff(freq_hz) <= 5_000 && level > dbm {
                dbm = level;
            }
        }
        for b in &self.bursts {
            if t_100us >= b.start_100us
                && t_100us < b.start_100us + b.dur_100us
                && b.freq_hz.abs_diff(freq_hz) <= 5_000
                && b.dbm > dbm
            {
                dbm = b.dbm;
            }
        }
        dbm
    }

    /// Subaudio index detectable at `freq_hz` (static table only).
    pub fn tone_at(&self, freq_hz: u32) -> Option<i32> {
        self.levels
            .iter()
            .find(|&&(hz, _, _)| hz.abs_diff(freq_hz) <= 5_000)
            .and_then(|&(_, _, tone)| tone)
    }

    /// RSSI (0..255) the FM chip would report at `khz`: the station's if
    /// within 100 kHz, else a low noise value.
    pub fn fm_rssi_at(&self, khz: u32) -> u8 {
        for &(s, rssi) in &self.fm_stations {
            if s.abs_diff(khz) <= 100 {
                return rssi;
            }
        }
        12
    }

    pub fn fm_is_station(&self, khz: u32) -> bool {
        self.fm_stations.iter().any(|&(s, _)| s.abs_diff(khz) <= 100)
    }
}
