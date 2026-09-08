//! Choosing what to boot.
//!
//! Deliberately plain: a numbered list, a countdown, and a key. A bootloader's
//! menu is the one piece of software you must be able to drive when everything
//! else on the machine is broken, so it depends on nothing but the console the
//! firmware already gave us.
//!
//! It does ask that console for one thing beyond writing text: the cursor. A
//! menu that reprints itself for every keypress, and a countdown that prints a
//! new line each second, turn the choice into a wall of scrollback — which is
//! not how anybody has drawn a menu since terminals could move the cursor.
//! Every line here has a fixed row and is rewritten where it stands.
//!
//! Firmware that will not position the cursor still gets the old behaviour.
//! Being usable on a bad machine is the point of a bootloader, and a scrolling
//! menu is worse to read but perfectly possible to drive.

use alloc::string::String;
use crate::config::Config;
use uefi::boot;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::{print, println, system};

/// Poll for a keypress without waiting for one.
fn poll_key() -> Option<Key> {
    system::with_stdin(|stdin| stdin.read_key().ok().flatten())
}

/// Where the menu's lines live, once it is on the screen.
///
/// Rows are absolute, so every redraw goes back to the same place. The list
/// starts wherever the cursor already was, which leaves the banner above it
/// rather than clearing the screen out from under whatever else has been said.
struct Layout {
    origin: usize,
    cols: usize,
    entries: usize,
}

impl Layout {
    fn title_row(&self) -> usize {
        self.origin
    }
    fn entry_row(&self, i: usize) -> usize {
        self.origin + 2 + i
    }
    fn hint_row(&self) -> usize {
        self.origin + 3 + self.entries
    }
    fn status_row(&self) -> usize {
        self.origin + 4 + self.entries
    }
    /// Rows from `origin` to the last one written, inclusive.
    fn height(&self) -> usize {
        5 + self.entries
    }

    /// Write one line where it belongs, blanking the rest of the row.
    ///
    /// Padded rather than cleared because there is no "erase to end of line"
    /// in the UEFI text protocol — only spaces. One column is left alone at
    /// the end: writing into the last one scrolls on some firmware, which
    /// would undo the whole point of this.
    fn line(&self, row: usize, text: &str) {
        let width = self.cols.saturating_sub(1);
        let mut s = String::with_capacity(width);
        for c in text.chars().take(width) {
            s.push(c);
        }
        while s.chars().count() < width {
            s.push(' ');
        }
        system::with_stdout(|out| {
            let _ = out.set_cursor_position(0, row);
        });
        print!("{}", s);
    }
}

/// Work out where the menu can go, if the console will let us place it at all.
///
/// `None` means fall back to printing: either the firmware would not say how
/// big its console is, would not move the cursor, or there are not enough rows
/// left below the cursor to hold the menu without scrolling it off.
fn plan(entries: usize) -> Option<Layout> {
    let (cols, rows) = system::with_stdout(|out| {
        out.current_mode().ok().flatten().map(|m| (m.columns(), m.rows()))
    })?;
    let origin = system::with_stdout(|out| out.cursor_position().1);
    let layout = Layout { origin, cols, entries };
    if cols < 20 || origin + layout.height() > rows {
        return None;
    }
    // Prove the cursor moves before drawing anything that depends on it.
    system::with_stdout(|out| out.set_cursor_position(0, origin)).ok()?;
    Some(layout)
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

    let layout = plan(cfg.entries.len());
    let mut selected = cfg.default;

    match &layout {
        Some(l) => {
            // The caret would sit in the middle of the list, blinking, since
            // nothing here is a text field.
            system::with_stdout(|out| {
                let _ = out.enable_cursor(false);
            });
            draw(l, cfg, selected);
        }
        None => render_scrolling(cfg, selected),
    }

    // Any key cancels the countdown: someone is here, so wait for them rather
    // than booting out from under them.
    let mut remaining = cfg.timeout;
    let mut counting = true;
    if let Some(l) = &layout {
        countdown(l, cfg, selected, remaining);
    }

    // Tenths of a second, so the countdown is honest and a key is noticed
    // promptly. Polling rather than waiting on the key event keeps both in one
    // loop without a timer protocol.
    let mut ticks = 0u64;

    let chosen = loop {
        if let Some(key) = poll_key() {
            // Somebody is watching, so stop the clock rather than booting out
            // from under them.
            if counting {
                counting = false;
                match &layout {
                    Some(l) => l.line(l.status_row(), "    (countdown cancelled)"),
                    None => println!("    (countdown cancelled)"),
                }
            }
            match key {
                Key::Special(ScanCode::UP) => {
                    selected = if selected == 0 { cfg.entries.len() - 1 } else { selected - 1 };
                    match &layout {
                        Some(l) => entries(l, cfg, selected),
                        None => render_scrolling(cfg, selected),
                    }
                }
                Key::Special(ScanCode::DOWN) => {
                    selected = (selected + 1) % cfg.entries.len();
                    match &layout {
                        Some(l) => entries(l, cfg, selected),
                        None => render_scrolling(cfg, selected),
                    }
                }
                Key::Printable(c) => {
                    let ch = char::from(c);
                    if ch == '\r' || ch == '\n' {
                        break selected;
                    }
                    // A digit picks an entry directly, which is what anyone
                    // reading a numbered list tries first.
                    if let Some(d) = ch.to_digit(10) {
                        let i = d as usize;
                        if i >= 1 && i <= cfg.entries.len() {
                            break i - 1;
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
                    break selected;
                }
                match &layout {
                    Some(l) => countdown(l, cfg, selected, remaining),
                    None => {
                        println!(
                            "    booting \"{}\" in {}s ...",
                            cfg.entries[selected].title, remaining
                        )
                    }
                }
            }
        }

        boot::stall(core::time::Duration::from_millis(100));
    };

    // Leave the console as we found it, with the cursor below the menu rather
    // than in the middle of it: everything after this prints normally.
    if let Some(l) = &layout {
        system::with_stdout(|out| {
            let _ = out.enable_cursor(true);
            let _ = out.set_cursor_position(0, l.status_row() + 1);
        });
    }
    chosen
}

/// The whole menu, once.
fn draw(l: &Layout, cfg: &Config, selected: usize) {
    l.line(l.title_row(), "    Select what to boot:");
    l.line(l.title_row() + 1, "");
    entries(l, cfg, selected);
    l.line(l.entry_row(cfg.entries.len()), "");
    l.line(l.hint_row(), "    up/down to move, a number to pick, enter to boot");
    l.line(l.status_row(), "");
}

/// Just the list, which is all that changes when the selection moves.
fn entries(l: &Layout, cfg: &Config, selected: usize) {
    for (i, e) in cfg.entries.iter().enumerate() {
        let mark = if i == selected { '>' } else { ' ' };
        let mut s = String::from("     ");
        s.push(mark);
        s.push(' ');
        push_num(&mut s, i + 1);
        s.push('.');
        s.push(' ');
        s.push_str(&e.title);
        l.line(l.entry_row(i), &s);
    }
}

fn countdown(l: &Layout, cfg: &Config, selected: usize, remaining: u64) {
    let mut s = String::from("    booting \"");
    s.push_str(&cfg.entries[selected].title);
    s.push_str("\" in ");
    push_num(&mut s, remaining as usize);
    s.push_str("s ...");
    l.line(l.status_row(), &s);
}

fn push_num(s: &mut String, mut n: usize) {
    if n == 0 {
        s.push('0');
        return;
    }
    let mut digits = [0u8; 20];
    let mut len = 0;
    while n > 0 {
        digits[len] = b'0' + (n % 10) as u8;
        n /= 10;
        len += 1;
    }
    while len > 0 {
        len -= 1;
        s.push(digits[len] as char);
    }
}

/// What a console that will not move its cursor gets: the menu again, below
/// the last one. Ugly, and legible, which is the right order for a bootloader.
fn render_scrolling(cfg: &Config, selected: usize) {
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
