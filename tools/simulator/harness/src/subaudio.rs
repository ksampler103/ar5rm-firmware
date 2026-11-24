//! Subaudio (CTCSS/DCS) conversions, ported from the firmware's
//! `src/app/convert.rs` + `src/app/settings.rs` tables (bf-5rh, GPL-3.0),
//! so the simulator's `subaudio_*` API calls answer exactly like the
//! radio's. The "code" encoding is `flash_map::SubaudioCode`: 0 = none,
//! 1..=105 = DCS normal (1-based table index), 106..=250 = DCS inverted
//! (index = code - 105), >250 = CTCSS in tenths of a Hz.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SubAudio {
    None,
    Ctcss(u16), // tenths of Hz
    Dcs { code: u16, inverted: bool },
}

pub const CTCSS_TABLE: [u16; 50] = [
    670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148,
    1188, 1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738,
    1773, 1799, 1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336,
    2418, 2503, 2541,
];

pub const DCS_TABLE: [u16; 105] = [
    0o023, 0o025, 0o026, 0o031, 0o032, 0o036, 0o043, 0o047, 0o051, 0o053, 0o054, 0o065, 0o071,
    0o072, 0o073, 0o074, 0o114, 0o115, 0o116, 0o122, 0o125, 0o131, 0o132, 0o134, 0o143, 0o145,
    0o152, 0o155, 0o156, 0o162, 0o165, 0o172, 0o174, 0o205, 0o212, 0o223, 0o225, 0o226, 0o243,
    0o244, 0o245, 0o246, 0o251, 0o252, 0o255, 0o261, 0o263, 0o265, 0o266, 0o271, 0o274, 0o306,
    0o311, 0o315, 0o325, 0o331, 0o332, 0o343, 0o346, 0o351, 0o356, 0o364, 0o365, 0o371, 0o411,
    0o412, 0o413, 0o423, 0o431, 0o432, 0o445, 0o446, 0o452, 0o454, 0o455, 0o462, 0o464, 0o465,
    0o466, 0o503, 0o506, 0o516, 0o523, 0o526, 0o532, 0o546, 0o565, 0o606, 0o612, 0o624, 0o627,
    0o631, 0o632, 0o645, 0o654, 0o662, 0o664, 0o703, 0o712, 0o723, 0o731, 0o732, 0o734, 0o743,
    0o754,
];

pub fn ctcss_index(tenths_hz: u16) -> Option<usize> {
    CTCSS_TABLE.iter().position(|&t| t == tenths_hz)
}

pub fn dcs_index(code: u16) -> Option<usize> {
    DCS_TABLE.iter().position(|&c| c == code)
}

pub fn from_code(code: u16) -> SubAudio {
    if code > 250 {
        SubAudio::Ctcss(code)
    } else if code >= 1 {
        if code <= 105 {
            dcs_from_table_index(code, false)
        } else {
            dcs_from_table_index(code - 105, true)
        }
    } else {
        SubAudio::None
    }
}

fn dcs_from_table_index(idx: u16, inverted: bool) -> SubAudio {
    match DCS_TABLE.get((idx as usize).wrapping_sub(1)) {
        Some(&code) => SubAudio::Dcs { code, inverted },
        None => SubAudio::None,
    }
}

pub fn to_code(sub: SubAudio) -> u16 {
    match sub {
        SubAudio::None => 0,
        SubAudio::Ctcss(t) => t,
        SubAudio::Dcs { code, inverted } => match dcs_index(code) {
            Some(i) => {
                let one_based = i as u16 + 1;
                if inverted {
                    one_based + 105
                } else {
                    one_based
                }
            }
            None => 0,
        },
    }
}

pub fn from_index(v: i32) -> SubAudio {
    if v < 0 {
        return SubAudio::None;
    }
    let v = v as usize;
    if v < CTCSS_TABLE.len() {
        return SubAudio::Ctcss(CTCSS_TABLE[v]);
    }
    let dcs_v = v - CTCSS_TABLE.len();
    if dcs_v < DCS_TABLE.len() {
        SubAudio::Dcs {
            code: DCS_TABLE[dcs_v],
            inverted: false,
        }
    } else {
        let inv_v = (dcs_v - DCS_TABLE.len()).min(DCS_TABLE.len() - 1);
        SubAudio::Dcs {
            code: DCS_TABLE[inv_v],
            inverted: true,
        }
    }
}

pub fn index_of(sub: SubAudio) -> i32 {
    match sub {
        SubAudio::None => -1,
        SubAudio::Ctcss(hz) => match ctcss_index(hz) {
            Some(i) => i as i32,
            None => -1,
        },
        SubAudio::Dcs { code, inverted } => match dcs_index(code) {
            Some(i) => {
                let base = CTCSS_TABLE.len() + if inverted { DCS_TABLE.len() } else { 0 };
                (base + i) as i32
            }
            None => -1,
        },
    }
}
