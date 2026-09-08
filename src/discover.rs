//! Finding what is installed, rather than being told.
//!
//! A config file can only boot what somebody wrote down. That is enough for an
//! appliance and not enough for a bootloader: the machine already knows what is
//! on it, and a loader that cannot say so leaves the user editing a file with
//! the one tool they cannot start.
//!
//! So every filesystem the firmware can see is looked at, not just the one Bang
//! was loaded from, and the well-known places a loader installs itself are
//! checked. This is deliberately shallow. It reads directory names under `\EFI`
//! and looks for a handful of filenames; it does not parse anybody's
//! configuration, mount ext4, or guess at what an unfamiliar binary might be.
//! Everything it finds is chainloaded, which is the operation that needs to
//! know nothing about what it is starting.
//!
//! What it finds is offered *alongside* the config rather than instead of it —
//! and `autodetect off` turns it off, because a bootloader that insists on
//! adding entries you did not ask for is one you end up fighting.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use uefi::boot::{self, SearchType};
use uefi::proto::device_path::DevicePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{Directory, File, FileAttribute, FileMode, FileType};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::{cstr16, CStr16, CString16, Handle, Identify, Status};

/// A loader found on some volume.
pub struct Found {
    pub title: String,
    /// Which volume it is on. Chainloading needs this: a file path means
    /// nothing without the device it is relative to, and the whole point here
    /// is that these are not all on our own.
    pub device: Handle,
    pub path: CString16,
}

/// Windows, at the one path it always uses.
const WINDOWS: &CStr16 = cstr16!("\\EFI\\Microsoft\\Boot\\bootmgfw.efi");

/// Filenames that mean "a bootloader lives here", in the order we would rather
/// find them. `shim` before `grub` because where both exist, shim is the one
/// meant to be started.
const LOADERS: &[&CStr16] = &[
    cstr16!("shimx64.efi"),
    cstr16!("grubx64.efi"),
    cstr16!("systemd-bootx64.efi"),
    cstr16!("refind_x64.efi"),
    cstr16!("BOOTX64.EFI"),
];

/// Everything bootable the firmware can reach.
pub fn scan() -> Vec<Found> {
    let mut out = Vec::new();

    let Ok(handles) = boot::locate_handle_buffer(SearchType::ByProtocol(&SimpleFileSystem::GUID))
    else {
        return out;
    };

    let own = own_image();

    for &device in handles.iter() {
        // Skipped rather than reported: a volume the firmware lists but will
        // not hand over is not an error, it is a volume somebody else is
        // using. Refusing to boot over it would be the wrong answer.
        let Ok(mut fs) = boot::open_protocol_exclusive::<SimpleFileSystem>(device) else {
            continue;
        };
        let Ok(mut root) = fs.open_volume() else {
            continue;
        };
        probe(device, &mut root, own.as_ref(), &mut out);
    }

    out
}

/// Where Bang itself was loaded from, so it does not offer to boot itself.
fn own_image() -> Option<(Handle, CString16)> {
    let loaded = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()).ok()?;
    let device = loaded.device()?;
    let path = loaded.file_path()?;
    Some((device, file_path_of(path)?))
}

/// The filename part of a device path, which is what a loader is named by.
fn file_path_of(path: &DevicePath) -> Option<CString16> {
    use uefi::proto::device_path::text::{AllowShortcuts, DisplayOnly};
    path.to_string(DisplayOnly(false), AllowShortcuts(false)).ok()
}

fn probe(device: Handle, root: &mut Directory, own: Option<&(Handle, CString16)>, out: &mut Vec<Found>) {
    if exists(root, WINDOWS) {
        out.push(Found {
            title: "Windows Boot Manager".to_string(),
            device,
            path: WINDOWS.into(),
        });
    }

    // Everything else that has put a loader under \EFI. Reading the directory
    // rather than trying a list of distribution names: the list is endless and
    // the directory is right in front of us.
    let Ok(handle) = root.open(cstr16!("\\EFI"), FileMode::Read, FileAttribute::empty()) else {
        return;
    };
    let Ok(FileType::Dir(mut efi)) = handle.into_type() else {
        return;
    };

    let mut buf = alloc::vec![0u8; 512];
    loop {
        let entry = match efi.read_entry(&mut buf) {
            Ok(Some(info)) => info,
            Ok(None) => break,
            Err(e) => {
                // The name was longer than the buffer. Grow once and retry
                // rather than losing the entry.
                if e.status() == Status::BUFFER_TOO_SMALL {
                    if let Some(needed) = e.data() {
                        buf.resize(*needed, 0);
                        continue;
                    }
                }
                break;
            }
        };
        if !entry.is_directory() {
            continue;
        }
        let name = entry.file_name();
        if name == cstr16!(".") || name == cstr16!("..") || name == cstr16!("Microsoft") {
            continue; // handled above, or not a directory worth descending
        }

        // `name` borrows the buffer, so anything kept has to be copied out
        // before the next entry is read into it.
        let vendor: CString16 = name.into();
        for loader in LOADERS {
            let mut path = CString16::try_from("\\EFI\\").unwrap();
            path.push_str(&vendor);
            path.push_str(cstr16!("\\"));
            path.push_str(loader);
            if !exists(root, &path) {
                continue;
            }
            if own.is_some_and(|(d, p)| *d == device && same_file(p, &path)) {
                break; // this is Bang
            }
            out.push(Found { title: pretty(&vendor, loader), device, path });
            break; // one loader per vendor directory is enough
        }
    }
}

fn exists(root: &mut Directory, path: &CStr16) -> bool {
    match root.open(path, FileMode::Read, FileAttribute::empty()) {
        Ok(h) => {
            // A directory of that name is not a loader.
            matches!(h.into_type(), Ok(FileType::Regular(_)))
        }
        Err(_) => false,
    }
}

/// Whether a device path's text ends with this file path.
///
/// The loaded-image path is the whole thing from the PCI root down; ours is
/// just the file. Comparing the tail is what says "the same file".
fn same_file(device_path_text: &CString16, file: &CString16) -> bool {
    let hay: String = device_path_text.to_string();
    let needle: String = file.to_string();
    hay.to_ascii_uppercase().ends_with(&needle.to_ascii_uppercase())
}

/// What to call it. The directory is usually the distribution — `fedora` is
/// Fedora — but two of these are boot managers that install under their own
/// name, and calling them by their directory would name the wrong thing.
fn pretty(vendor: &CString16, loader: &CStr16) -> String {
    let file: String = loader.to_string();
    if file.eq_ignore_ascii_case("systemd-bootx64.efi") {
        return "systemd-boot".to_string();
    }
    if file.eq_ignore_ascii_case("refind_x64.efi") {
        return "rEFInd".to_string();
    }
    let name: String = vendor.to_string();
    if name.eq_ignore_ascii_case("BOOT") {
        return "EFI default loader".to_string();
    }
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    out
}
