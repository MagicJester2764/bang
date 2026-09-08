//! Choosing what to boot.
//!
//! Deliberately plain: a numbered list, a countdown, and a key. A bootloader's
//! menu is the one piece of software you must be able to drive when everything
//! else on the machine is broken, so it depends on nothing but the console the
//! firmware already gave us.

use crate::config::Config;
use uefi::boot;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::{println, system};

/// Poll for a keypress without waiting for one.
fn poll_key() -> Option<Key> {
    system::with_stdin(|stdin| stdin.read_key().ok().flatten())
}

/// Ask which entry to boot, returning its index.
///
/// Returns the default at once when there is nothing to choose between, or
/// when the timeout is zero — an unattended machine should not stop at a menu
/// it has only one answer for.
pub fn choose(cfg: &Config) -> usize {
    if cfg.entries.len() <= 1 || cfg.timeout == 0 {
        return cfg.default;
    }

    let mut selected = cfg.default;
    render(cfg, selected);

    // Any key cancels the countdown: someone is here, so wait for them rather
    // than booting out from under them.
    let mut remaining = cfg.timeout;
    let mut counting = true;

    // Tenths of a second, so the countdown is honest and a key is noticed
    // promptly. Polling rather than waiting on the key event keeps both in one
    // loop without a timer protocol.
    let mut ticks = 0u64;

    loop {
        if let Some(key) = poll_key() {
            // Somebody is watching, so stop the clock rather than booting out
            // from under them.
            if counting {
                counting = false;
                println!("    (countdown cancelled)");
            }
            match key {
                Key::Special(ScanCode::UP) => {
                    selected = if selected == 0 { cfg.entries.len() - 1 } else { selected - 1 };
                    render(cfg, selected);
                }
                Key::Special(ScanCode::DOWN) => {
                    selected = (selected + 1) % cfg.entries.len();
                    render(cfg, selected);
                }
                Key::Printable(c) => {
                    let ch = char::from(c);
                    if ch == '\r' || ch == '\n' {
                        return selected;
                    }
                    // A digit picks an entry directly, which is what anyone
                    // reading a numbered list tries first.
                    if let Some(d) = ch.to_digit(10) {
                        let i = d as usize;
                        if i >= 1 && i <= cfg.entries.len() {
                            return i - 1;
                        }
                    }
                }
                _ => {}
            }
        }

        if counting {
            ticks += 1;
            if ticks % 10 == 0 {
                remaining -= 1;
                if remaining == 0 {
                    return selected;
                }
                println!("    booting \"{}\" in {}s ...", cfg.entries[selected].title, remaining);
            }
        }

        boot::stall(core::time::Duration::from_millis(100));
    }
}

fn render(cfg: &Config, selected: usize) {
    println!();
    println!("    Select what to boot:");
    println!();
    for (i, e) in cfg.entries.iter().enumerate() {
        let mark = if i == selected { ">" } else { " " };
        println!("     {} {}. {}", mark, i + 1, e.title);
    }
    println!();
    println!("    up/down to move, a number to pick, enter to boot");
}
