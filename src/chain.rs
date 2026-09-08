//! Starting another UEFI application.
//!
//! This is the cheapest way for a bootloader to boot something it knows
//! nothing about: hand the image back to the firmware and let it run, exactly
//! as though the firmware had been asked for it directly. Windows arrives this
//! way — `\EFI\Microsoft\Boot\bootmgfw.efi` is an ordinary UEFI application —
//! and so does every other EFI-bootable system, GRUB and systemd-boot
//! included.
//!
//! The image is loaded by *device path*, not from a buffer. A loaded image is
//! told where it came from, and one that has to find its own files — which
//! `bootmgfw.efi` does, and a UEFI shell does not — has nothing to look
//! relative to otherwise.

use alloc::vec::Vec;
use uefi::boot::{self, LoadImageSource};
use uefi::proto::device_path::build::{media, DevicePathBuilder};
use uefi::proto::device_path::DevicePath;
use uefi::proto::loaded_image::LoadedImage;
use uefi::{println, CStr16, Status};

/// Build `<the volume we were loaded from>/<file>`.
///
/// The firmware wants a whole path, from the PCI root down to the file, so the
/// device half is taken from our own image and only the file node is new.
fn full_path<'a>(file: &CStr16, buf: &'a mut Vec<u8>) -> Option<&'a DevicePath> {
    let loaded = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()).ok()?;
    let device = loaded.device()?;
    let device_path = boot::open_protocol_exclusive::<DevicePath>(device).ok()?;

    let mut builder = DevicePathBuilder::with_vec(buf);
    // Everything but the end node: appending after it would leave the file
    // node past the end of the path, where nothing reads it.
    for node in device_path.node_iter() {
        if node.full_type() == (uefi::proto::device_path::DeviceType::END, uefi::proto::device_path::DeviceSubType::END_ENTIRE) {
            break;
        }
        builder = builder.push(&node).ok()?;
    }
    builder = builder.push(&media::FilePath { path_name: file }).ok()?;
    builder.finalize().ok()
}

/// Load and start `file`, which does not return if it succeeds.
///
/// Boot services are still up: the loaded image needs them, and exiting them
/// is its business rather than ours.
pub fn boot(file: &CStr16) -> Status {
    println!("[+] Chainloading {} ...", file);

    let mut buf = Vec::new();
    let Some(path) = full_path(file, &mut buf) else {
        println!("[!] Could not work out a device path for {}", file);
        return Status::NOT_FOUND;
    };

    let image = match boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromDevicePath {
            device_path: path,
            boot_policy: uefi::proto::BootPolicy::ExactMatch,
        },
    ) {
        Ok(h) => h,
        Err(e) => {
            println!("[!] Could not load {}: {:?}", file, e.status());
            return e.status();
        }
    };

    match boot::start_image(image) {
        // Reached only if the image returns, which a boot manager does when
        // the user backs out of it. Returning here puts them back in our menu.
        Ok(()) => {
            println!("[*] {} returned", file);
            Status::SUCCESS
        }
        Err(e) => {
            println!("[!] {} failed: {:?}", file, e.status());
            e.status()
        }
    }
}
