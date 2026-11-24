// Fallback directory, compiled in — used only when no valid RPBK
// data block is present in the app's flash slot. The full directory
// (the owner's channels, or a ZIP-built repeater list) is written
// into the slot at offset 0x6000 by the web flasher at install time;
// see gen_default_block.py and default_channels.json. Keep this
// table SMALL: a resident overlay segment must fit ARENA_SIZE
// (8192 B total), and this table used to be 165 entries (~4.6 KB),
// which is exactly what pushed the app into an oversized segment
// the loader refuses to keep resident (FAULT_OVERSIZE_CONTINUE).

pub struct BakedRec {
    pub name: [u8; 12],
    pub rx_hz: u32,
    pub tx_hz: u32,
    pub tx_tone: u16,
    pub rx_tone: u16,
    pub flags: u8,
}

pub static BAKED: &[BakedRec] = &[
    BakedRec { name: [87, 88, 49, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162550000, tx_hz: 162550000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX1
    BakedRec { name: [87, 88, 50, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162400000, tx_hz: 162400000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX2
    BakedRec { name: [87, 88, 51, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162475000, tx_hz: 162475000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX3
    BakedRec { name: [87, 88, 52, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162425000, tx_hz: 162425000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX4
    BakedRec { name: [87, 88, 53, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162450000, tx_hz: 162450000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX5
    BakedRec { name: [87, 88, 54, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162500000, tx_hz: 162500000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX6
    BakedRec { name: [87, 88, 55, 32, 32, 32, 32, 32, 32, 32, 32, 32], rx_hz: 162525000, tx_hz: 162525000, tx_tone: 0, rx_tone: 0, flags: 0x08 }, // WX7
    BakedRec { name: [71, 77, 82, 83, 32, 53, 53, 48, 32, 32, 32, 32], rx_hz: 462550000, tx_hz: 462550000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 550
    BakedRec { name: [71, 77, 82, 83, 32, 53, 55, 53, 32, 32, 32, 32], rx_hz: 462575000, tx_hz: 462575000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 575
    BakedRec { name: [71, 77, 82, 83, 32, 54, 48, 48, 32, 32, 32, 32], rx_hz: 462600000, tx_hz: 462600000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 600
    BakedRec { name: [71, 77, 82, 83, 32, 54, 50, 53, 32, 32, 32, 32], rx_hz: 462625000, tx_hz: 462625000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 625
    BakedRec { name: [71, 77, 82, 83, 32, 54, 53, 48, 32, 32, 32, 32], rx_hz: 462650000, tx_hz: 462650000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 650
    BakedRec { name: [71, 77, 82, 83, 32, 54, 55, 53, 32, 32, 32, 32], rx_hz: 462675000, tx_hz: 462675000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 675
    BakedRec { name: [71, 77, 82, 83, 32, 55, 48, 48, 32, 32, 32, 32], rx_hz: 462700000, tx_hz: 462700000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 700
    BakedRec { name: [71, 77, 82, 83, 32, 55, 50, 53, 32, 32, 32, 32], rx_hz: 462725000, tx_hz: 462725000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // GMRS 725
    BakedRec { name: [50, 77, 32, 67, 65, 76, 76, 32, 32, 32, 32, 32], rx_hz: 146520000, tx_hz: 146520000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // 2M CALL
    BakedRec { name: [55, 48, 67, 77, 32, 67, 65, 76, 76, 32, 32, 32], rx_hz: 446000000, tx_hz: 446000000, tx_tone: 0, rx_tone: 0, flags: 0x00 }, // 70CM CALL
];
