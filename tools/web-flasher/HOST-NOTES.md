# Web flasher — host notes (live findings, 2026-11-24)

## macOS + Prolific-clone cable + Web Serial: CPS backup does NOT work
Preston's cable is a Prolific PL2303-class Baofeng cable (`cu.usbserial-1120`,
"USB-Serial Controller D") used from Chrome on macOS (GitHub Pages copy).
Symptoms, fully logged: port opens clean at 115200 8N1, DTR+RTS asserted,
ident `PROGRAMBFNORMALU` sent byte-perfect (3 attempts, 5 s waits, CHIRP-mirrored
envelope) — radio never enters program mode, replies with zero bytes, and keys
its RF transmitter on its operating channel during ident bursts (the data line
is seen as PTT closures because the radio's UART never syncs). The bootloader
probe (flash mode) also got total silence on macOS (radio's flash-mode entry
in that run was not independently confirmed).

Control tests: CHIRP (model 5RM, pyserial) on the SAME Mac/cable/radio reads
the radio fine. The web page on **Windows** (Chrome, same cable/radio) read
the backup fine on 2026-11-24.

Conclusion: host-stack issue specific to macOS + this clone adapter + Web
Serial (baud/framing delivery suspected; not root-caused further). CHIRP's
wire protocol is byte-identical to the page's (see CHIRP-DIFF.md) — the page
is not the problem. **Use a Windows host for the web flasher with this cable.**
A genuine FTDI/CH340 cable might behave differently on macOS; untested.
