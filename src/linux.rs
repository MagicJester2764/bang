//! Booting Linux through the EFI handover protocol.
//!
//! A bzImage is two things stuck together: a few hundred bytes of *setup
//! header* describing the rest, then the compressed kernel. The header is a
//! contract that has grown by accretion since 1991, so the fields are at fixed
//! byte offsets and are read as such rather than as a struct — a `#[repr(C)]`
//! version of it would be a transcription exercise with one chance to get a
//! padding byte wrong.
//!
//! The handover protocol says: load the second part, fill in a `boot_params`
//! page from the first, and call an entry point the header names. From there
//! the kernel is a UEFI application in its own right — it calls
//! `ExitBootServices` itself, which is why this must not.
//!
//! ```text
//!   bzImage file          memory
//!   +----------------+    +------------------+
//!   | setup header   |--->| boot_params page |  the header, copied, plus
//!   | (from 0x1F1)   |    | (4 KiB, zeroed)  |  where everything else went
//!   +----------------+    +------------------+
//!   | protected-mode |--->| kernel image     |  init_size bytes, aligned
//!   | kernel         |    +------------------+
//!   +----------------+    | cmdline, initrd  |
//!                         +------------------+
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use uefi::{println, CStr16, Status};

// Setup header fields, by offset from the start of the file. Also their
// offsets within boot_params, which holds the header at the same place.
const SETUP_SECTS: usize = 0x1F1;
const BOOT_FLAG: usize = 0x1FE;
const HEADER_MAGIC: usize = 0x202;
const VERSION: usize = 0x206;
const TYPE_OF_LOADER: usize = 0x210;
const LOADFLAGS: usize = 0x211;
const CODE32_START: usize = 0x214;
const RAMDISK_IMAGE: usize = 0x218;
const RAMDISK_SIZE: usize = 0x21C;
const CMD_LINE_PTR: usize = 0x228;
const KERNEL_ALIGNMENT: usize = 0x230;
const RELOCATABLE: usize = 0x234;
const XLOADFLAGS: usize = 0x236;
const CMDLINE_SIZE: usize = 0x238;
const PREF_ADDRESS: usize = 0x258;
const INIT_SIZE: usize = 0x260;
const HANDOVER_OFFSET: usize = 0x264;

/// `boot_flag`, the last two bytes of the first sector.
const BOOT_FLAG_MAGIC: u16 = 0xAA55;
/// `"HdrS"`, which says there is a setup header worth reading.
const HDRS_MAGIC: u32 = 0x5372_6448;
/// The oldest protocol version with a handover entry point.
const MIN_VERSION: u16 = 0x020B;

/// `XLF_EFI_HANDOVER_64`: there is a 64-bit handover entry.
const XLF_EFI_HANDOVER_64: u16 = 1 << 3;

/// `LOADED_HIGH`: the kernel is loaded above the old 1 MiB line.
const LOADFLAG_LOADED_HIGH: u8 = 0x01;
/// `type_of_loader` for a loader with no assigned number.
const LOADER_UNKNOWN: u8 = 0xFF;

/// The 64-bit entry sits one sector past the 32-bit one.
const HANDOVER_64_OFFSET: usize = 512;

fn u8_at(b: &[u8], o: usize) -> u8 {
    b[o]
}
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut v = [0u8; 8];
    v.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(v)
}
fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Read a whole file off the boot volume.
///
/// Loops rather than trusting one call. `read` may return less than was asked
/// for, and a bzImage's handover entry is a few hundred bytes from the end of
/// eighteen megabytes: a short read leaves exactly the part that gets jumped
/// to as zeros, and the fault lands with nothing to say where it came from.
fn read_file(path: &CStr16) -> Option<Vec<u8>> {
    let mut fs = boot::get_image_file_system(boot::image_handle()).ok()?;
    let mut root = fs.open_volume().ok()?;
    let handle = root.open(path, FileMode::Read, FileAttribute::empty()).ok()?;
    let mut file = handle.into_regular_file()?;
    let size = file.get_boxed_info::<FileInfo>().ok()?.file_size() as usize;

    let mut buf = alloc::vec![0u8; size];
    let mut done = 0;
    while done < size {
        match file.read(&mut buf[done..]) {
            Ok(0) => break,
            Ok(n) => done += n,
            Err(_) => break,
        }
    }
    if done != size {
        println!("[!] {} is {} bytes but only {} could be read", path, size, done);
        return None;
    }
    Some(buf)
}

/// Allocate pages below 4 GiB.
///
/// The setup header's addresses for the command line and the ramdisk are
/// 32-bit. There are `ext_` fields for the halves above that, but nothing here
/// needs the space, and staying below the line means no chance of handing the
/// kernel an address with its top silently removed.
fn alloc_low(bytes: usize) -> Option<*mut u8> {
    let pages = bytes.div_ceil(4096).max(1);
    boot::allocate_pages(
        AllocateType::MaxAddress(0x1_0000_0000),
        MemoryType::LOADER_DATA,
        pages,
    )
    .ok()
    .map(|p| p.as_ptr())
}

/// Boot the bzImage at `kernel_path`. Does not return if it works.
pub fn boot(kernel_path: &CStr16, initrd_path: Option<&CStr16>, cmdline: &str) -> Status {
    println!("[+] Loading {} ...", kernel_path);
    let Some(image) = read_file(kernel_path) else {
        println!("[!] Could not read {}", kernel_path);
        return Status::NOT_FOUND;
    };

    // Enough of a file to hold a setup header at all.
    if image.len() < 0x1000 {
        println!("[!] {} is too small to be a bzImage", kernel_path);
        return Status::LOAD_ERROR;
    }
    if u16_at(&image, BOOT_FLAG) != BOOT_FLAG_MAGIC || u32_at(&image, HEADER_MAGIC) != HDRS_MAGIC {
        println!("[!] {} is not a bzImage", kernel_path);
        return Status::LOAD_ERROR;
    }

    let version = u16_at(&image, VERSION);
    if version < MIN_VERSION {
        println!("[!] boot protocol {:#06x} is too old for handover", version);
        return Status::UNSUPPORTED;
    }
    let xloadflags = u16_at(&image, XLOADFLAGS);
    if xloadflags & XLF_EFI_HANDOVER_64 == 0 {
        println!("[!] this kernel has no 64-bit EFI handover entry");
        return Status::UNSUPPORTED;
    }
    let handover_offset = u32_at(&image, HANDOVER_OFFSET) as usize;

    // Setup occupies setup_sects sectors after the first; 0 means the ancient
    // default of 4. The kernel proper starts after all of them.
    let setup_sects = match u8_at(&image, SETUP_SECTS) {
        0 => 4usize,
        n => n as usize,
    };
    let kernel_offset = (setup_sects + 1) * 512;
    if kernel_offset >= image.len() {
        println!("[!] setup claims {} sectors, past the end of the file", setup_sects);
        return Status::LOAD_ERROR;
    }
    let kernel_bytes = &image[kernel_offset..];
    println!(
        "[+] {} setup sectors, {} bytes of kernel, {} bytes needed in memory",
        setup_sects,
        kernel_bytes.len(),
        u32_at(&image, INIT_SIZE)
    );

    // init_size is what the kernel needs in memory, which is more than it
    // occupies in the file: it decompresses into the same region.
    let init_size = u32_at(&image, INIT_SIZE) as usize;
    let alignment = u32_at(&image, KERNEL_ALIGNMENT) as usize;
    let relocatable = u8_at(&image, RELOCATABLE) != 0;
    let pref_address = u64_at(&image, PREF_ADDRESS);

    let need = init_size.max(kernel_bytes.len());
    let kernel_mem = if relocatable {
        match alloc_low(need + alignment) {
            Some(p) => {
                // Round up by hand: the allocator promises page alignment and
                // the kernel wants rather more than that.
                let addr = p as usize;
                let aligned = if alignment > 1 { addr.next_multiple_of(alignment) } else { addr };
                aligned as *mut u8
            }
            None => {
                println!("[!] could not allocate {} bytes for the kernel", need);
                return Status::OUT_OF_RESOURCES;
            }
        }
    } else {
        // Not relocatable: it has to go where it says, or not at all.
        match boot::allocate_pages(
            AllocateType::Address(pref_address),
            MemoryType::LOADER_DATA,
            need.div_ceil(4096),
        ) {
            Ok(p) => p.as_ptr(),
            Err(_) => {
                println!("[!] kernel is not relocatable and {:#x} is taken", pref_address);
                return Status::OUT_OF_RESOURCES;
            }
        }
    };

    unsafe {
        core::ptr::write_bytes(kernel_mem, 0, need);
        core::ptr::copy_nonoverlapping(kernel_bytes.as_ptr(), kernel_mem, kernel_bytes.len());
    }

    // boot_params: a zeroed page with the setup header copied into it at the
    // same offsets it had in the file.
    let Some(params) = alloc_low(4096) else {
        println!("[!] could not allocate boot_params");
        return Status::OUT_OF_RESOURCES;
    };
    let bp = unsafe {
        core::ptr::write_bytes(params, 0, 4096);
        core::slice::from_raw_parts_mut(params, 4096)
    };
    // The header runs from setup_sects to 0x202 plus the byte at 0x201, which
    // is the displacement of the jump that opens the boot sector and so
    // doubles as "where the header stops".
    let hdr_end = (HEADER_MAGIC + image[0x201] as usize).min(image.len()).min(4096);
    bp[SETUP_SECTS..hdr_end].copy_from_slice(&image[SETUP_SECTS..hdr_end]);

    // The command line, as NUL-terminated ASCII the kernel can find.
    let mut cl = String::from(cmdline);
    let max = u32_at(&image, CMDLINE_SIZE) as usize;
    if max > 0 && cl.len() > max {
        println!("[!] command line truncated to {} bytes", max);
        cl.truncate(max);
    }
    let Some(cmd_mem) = alloc_low(cl.len() + 1) else {
        println!("[!] could not allocate the command line");
        return Status::OUT_OF_RESOURCES;
    };
    unsafe {
        core::ptr::copy_nonoverlapping(cl.as_ptr(), cmd_mem, cl.len());
        *cmd_mem.add(cl.len()) = 0;
    }
    put_u32(bp, CMD_LINE_PTR, cmd_mem as u32);

    if let Some(path) = initrd_path {
        println!("[+] Loading {} ...", path);
        match read_file(path) {
            Some(initrd) => match alloc_low(initrd.len()) {
                Some(mem) => {
                    unsafe {
                        core::ptr::copy_nonoverlapping(initrd.as_ptr(), mem, initrd.len());
                    }
                    put_u32(bp, RAMDISK_IMAGE, mem as u32);
                    put_u32(bp, RAMDISK_SIZE, initrd.len() as u32);
                }
                None => println!("[!] could not allocate {} bytes for the initrd", initrd.len()),
            },
            // Booting without it is worth trying: an initrd is not always
            // needed, and the kernel says more about what went wrong than a
            // refusal here would.
            None => println!("[!] could not read {}; booting without it", path),
        }
    }

    bp[TYPE_OF_LOADER] = LOADER_UNKNOWN;
    bp[LOADFLAGS] |= LOADFLAG_LOADED_HIGH;
    put_u32(bp, CODE32_START, kernel_mem as u32);

    // The 64-bit entry is one sector past the 32-bit one, both counted from
    // the start of the loaded kernel rather than from the file.
    let entry = kernel_mem as usize + handover_offset + HANDOVER_64_OFFSET;
    println!(
        "[+] Handing over to Linux at {:#x} (protocol {}.{:02})",
        entry,
        version >> 8,
        version & 0xFF
    );

    // Boot services stay up: the kernel exits them itself, which is the whole
    // point of the handover protocol.
    //
    // `sysv64`, not `efiapi`. The handover entry is an interface Linux defines
    // rather than one UEFI does, so it takes its arguments the way the kernel
    // was compiled to — in rdi, rsi and rdx — and not the way every other
    // function reached from a UEFI application does. Calling it with the UEFI
    // convention puts the system table where it looks for `boot_params` and
    // leaves the other two as whatever happened to be in the registers, which
    // faults on a wild address inside the stub with nothing to say why.
    let handover: extern "sysv64" fn(uefi::Handle, *mut core::ffi::c_void, *mut u8) -> ! =
        unsafe { core::mem::transmute(entry) };
    handover(
        boot::image_handle(),
        uefi::table::system_table_raw()
            .map(|p| p.as_ptr().cast())
            .unwrap_or(core::ptr::null_mut()),
        params,
    );
}
