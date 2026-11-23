#![cfg_attr(windows, windows_subsystem = "windows")]

// Track the precompiled resource so Cargo relinks when the icon changes.
#[cfg(windows)]
const _: &[u8] = include_bytes!("../ShotSeries.res");

mod geometry;
#[cfg(windows)] mod controls;
#[cfg(windows)] mod desktop;
#[cfg(windows)] mod overlay;
#[cfg(windows)] mod storage;
#[cfg(windows)] mod windows;

#[cfg(windows)]
fn main() { windows::run(); }

#[cfg(not(windows))]
fn main() { eprintln!("ShotSeries desktop builds for Windows."); }
