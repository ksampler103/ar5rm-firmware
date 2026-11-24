//! bf5rh-sim — host simulator for bf-5rh overlay apps.
//!
//! An overlay app never touches hardware: everything it does goes
//! through the `bf5rh_abi::Api` vtable the firmware hands to its
//! `app_entry`. This crate implements that vtable on a PC — drawing into
//! a 160x128 RGB565 framebuffer, answering radio/settings/channel calls
//! from mock state seeded with the owner's real data, and feeding the
//! app scripted key/tick events — so apps can be run, screenshotted and
//! tested without the radio. See README.md in tools/simulator/.

pub mod api;
pub mod fb;
pub mod fwui;
pub mod rf;
pub mod scenario;
pub mod state;
pub mod subaudio;

pub use scenario::main_for;
