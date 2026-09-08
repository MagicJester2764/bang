//! What to boot, and from where.
//!
//! Bang loaded one kernel from one hardcoded path. A bootloader has to be told
//! what its choices are, so it reads `\bang.cfg` off the volume it was itself
//! loaded from. Without one it behaves exactly as it did before — a single
//! Multiboot2 entry for `\kernel.bin` — so an image built before this still
//! boots.
//!
//! The format is lines of `directive argument`, blank lines and `#` comments
//! ignored:
//!
//! ```text
//!     timeout 5
//!     default Quark
//!
//!     entry Quark
//!         multiboot \kernel.bin
//!         modules   \drivers
//!
//!     entry UEFI Shell
//!         chainload \EFI\tools\Shell.efi
//!
//!     entry Fedora
//!         linux   \vmlinuz
//!         initrd  \initrd.img
//!         options root=/dev/sda2 ro
//! ```
//!
//! Anything unrecognised is reported and skipped rather than refused. A
//! bootloader that will not start because of one bad line in its config is a
//! bootloader you cannot fix without another one.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use uefi::boot;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::{cstr16, println, CString16};

/// How an entry is started.
pub enum Target {
    /// A Multiboot2 or Multiboot1 kernel, with optional modules — Quark.
    Multiboot {
        kernel: CString16,
        modules: Option<CString16>,
    },
    /// Another UEFI application, started as though the firmware had. This is
    /// what boots Windows: `\EFI\Microsoft\Boot\bootmgfw.efi` is one.
    ///
    /// `device` is the volume it lives on, and `None` means the one Bang was
    /// loaded from — which is all a config file can name, since it has no way
    /// to write down a handle. Anything found by looking around says which
    /// disk it was found on.
    Chainload {
        path: CString16,
        device: Option<uefi::Handle>,
    },
    /// A Linux bzImage through the EFI handover protocol.
    Linux {
        kernel: CString16,
        initrd: Option<CString16>,
        cmdline: String,
    },
}

pub struct Entry {
    pub title: String,
    pub target: Target,
}

pub struct Config {
    /// Seconds to wait before booting the default. 0 boots at once.
    pub timeout: u64,
    pub default: usize,
    pub entries: Vec<Entry>,
    /// Whether to look around for loaders the config does not name.
    pub autodetect: bool,
}

impl Config {
    /// What Bang did before it could be configured.
    pub fn builtin() -> Self {
        Config {
            timeout: 0,
            default: 0,
            autodetect: true,
            entries: alloc::vec![Entry {
                title: "Quark".to_string(),
                target: Target::Multiboot {
                    kernel: CString16::try_from("\\kernel.bin").unwrap(),
                    modules: Some(CString16::try_from("\\drivers").unwrap()),
                },
            }],
        }
    }
}

/// Read and parse `\bang.cfg`, or `None` if there is not one.
pub fn load() -> Option<Config> {
    let mut fs = boot::get_image_file_system(boot::image_handle()).ok()?;
    let mut root = fs.open_volume().ok()?;
    let handle = root
        .open(cstr16!("\\bang.cfg"), FileMode::Read, FileAttribute::empty())
        .ok()?;
    let mut file = handle.into_regular_file()?;

    let size = file.get_boxed_info::<FileInfo>().ok()?.file_size() as usize;
    // A config is a page or two of text. A file claiming to be enormous is
    // not one, and reading it would be the bootloader's own fault.
    if size == 0 || size > 64 * 1024 {
        println!("[!] \\bang.cfg is {} bytes; ignoring it", size);
        return None;
    }

    let mut buf = alloc::vec![0u8; size];
    let read = file.read(&mut buf).ok()?;
    buf.truncate(read);

    let text = core::str::from_utf8(&buf).ok().or_else(|| {
        println!("[!] \\bang.cfg is not valid UTF-8");
        None
    })?;

    Some(parse(text))
}

/// Split a line into its directive and the rest, both trimmed.
fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    }
}

/// A path from the config, as UEFI wants it.
fn path(arg: &str) -> Option<CString16> {
    match CString16::try_from(arg) {
        Ok(p) => Some(p),
        Err(_) => {
            println!("[!] bang.cfg: cannot use path \"{}\"", arg);
            None
        }
    }
}

pub fn parse(text: &str) -> Config {
    let mut cfg = Config { timeout: 5, default: 0, entries: Vec::new(), autodetect: true };
    // The default may name an entry declared later, so it is resolved at the
    // end rather than as it is read.
    let mut default_title: Option<String> = None;

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (directive, arg) = split(line);

        match directive {
            "timeout" => cfg.timeout = arg.parse().unwrap_or(5),
            // On unless told otherwise: a machine with no config at all should
            // still offer what is installed on it.
            "autodetect" => cfg.autodetect = !matches!(arg, "off" | "no" | "false" | "0"),
            "default" => {
                // A number selects by position, anything else by title.
                match arg.parse::<usize>() {
                    Ok(n) => cfg.default = n,
                    Err(_) => default_title = Some(arg.to_string()),
                }
            }
            "entry" => cfg.entries.push(Entry {
                title: arg.to_string(),
                // Until a directive says otherwise. An entry that never gets
                // one is dropped below rather than booted into nothing.
                target: Target::Multiboot { kernel: CString16::new(), modules: None },
            }),
            _ => {
                let Some(entry) = cfg.entries.last_mut() else {
                    println!("[!] bang.cfg: \"{}\" before any entry", directive);
                    continue;
                };
                match directive {
                    "multiboot" => {
                        if let Some(p) = path(arg) {
                            entry.target = Target::Multiboot { kernel: p, modules: None };
                        }
                    }
                    "modules" => match &mut entry.target {
                        Target::Multiboot { modules, .. } => *modules = path(arg),
                        _ => println!("[!] bang.cfg: modules only apply to a multiboot entry"),
                    },
                    "chainload" => {
                        if let Some(p) = path(arg) {
                            entry.target = Target::Chainload { path: p, device: None };
                        }
                    }
                    "linux" => {
                        if let Some(p) = path(arg) {
                            entry.target = Target::Linux {
                                kernel: p,
                                initrd: None,
                                cmdline: String::new(),
                            };
                        }
                    }
                    "initrd" => match &mut entry.target {
                        Target::Linux { initrd, .. } => *initrd = path(arg),
                        _ => println!("[!] bang.cfg: initrd only applies to a linux entry"),
                    },
                    "options" => match &mut entry.target {
                        Target::Linux { cmdline, .. } => *cmdline = arg.to_string(),
                        _ => println!("[!] bang.cfg: options only apply to a linux entry"),
                    },
                    other => println!("[!] bang.cfg: ignoring \"{}\"", other),
                }
            }
        }
    }

    // An entry whose target was never filled in names nothing to boot.
    cfg.entries.retain(|e| match &e.target {
        Target::Multiboot { kernel, .. } => !kernel.is_empty(),
        _ => true,
    });

    if let Some(title) = default_title {
        match cfg.entries.iter().position(|e| e.title == title) {
            Some(i) => cfg.default = i,
            None => println!("[!] bang.cfg: no entry named \"{}\"", title),
        }
    }
    if cfg.default >= cfg.entries.len() {
        cfg.default = 0;
    }

    cfg
}
