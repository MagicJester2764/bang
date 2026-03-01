use core::ptr;
use uefi::boot;
use uefi::mem::memory_map::MemoryType;
use uefi::println;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};

const EI_NIDENT: usize = 16;
const ET_EXEC: u16 = 2;
const EM_386: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const ELFCLASS32: u8 = 1;
const ELFCLASS64: u8 = 2;

const MB1_HEADER_MAGIC: u32 = 0x1BAD_B002;
const MB2_HEADER_MAGIC: u32 = 0xE852_50D6;

/// Boot mode determined by ELF class and multiboot headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// Multiboot1 or Multiboot2 kernel — use 32-bit trampoline.
    Protected32,
    /// ELF64 kernel without multiboot — use 64-bit direct handoff.
    Long64,
}

/// Result of loading a kernel.
pub struct KernelInfo {
    pub entry_point: u64,
    pub mb_version: u32,
    pub boot_mode: BootMode,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf32Ehdr {
    e_ident: [u8; EI_NIDENT],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u32,
    e_phoff: u32,
    e_shoff: u32,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf32Phdr {
    p_type: u32,
    p_offset: u32,
    p_vaddr: u32,
    p_paddr: u32,
    p_filesz: u32,
    p_memsz: u32,
    p_flags: u32,
    p_align: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; EI_NIDENT],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct Elf64Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

/// Scan raw ELF file for multiboot headers.
/// Returns 2 (MB2), 1 (MB1), or 0 (none).
fn detect_multiboot_version(file_buf: &[u8]) -> u32 {
    // MB2: magic 0xE85250D6 within first 32768 bytes, 8-byte aligned
    let mb2_limit = file_buf.len().min(32768);
    let mut off = 0;
    while off + 16 <= mb2_limit {
        let magic = u32::from_le_bytes(file_buf[off..off + 4].try_into().unwrap());
        if magic == MB2_HEADER_MAGIC {
            let arch = u32::from_le_bytes(file_buf[off + 4..off + 8].try_into().unwrap());
            let hlen = u32::from_le_bytes(file_buf[off + 8..off + 12].try_into().unwrap());
            let chksum = u32::from_le_bytes(file_buf[off + 12..off + 16].try_into().unwrap());
            if magic.wrapping_add(arch).wrapping_add(hlen).wrapping_add(chksum) == 0 {
                return 2;
            }
        }
        off += 8;
    }

    // MB1: magic 0x1BADB002 within first 8192 bytes, 4-byte aligned
    let mb1_limit = file_buf.len().min(8192);
    off = 0;
    while off + 12 <= mb1_limit {
        let magic = u32::from_le_bytes(file_buf[off..off + 4].try_into().unwrap());
        if magic == MB1_HEADER_MAGIC {
            let flags = u32::from_le_bytes(file_buf[off + 4..off + 8].try_into().unwrap());
            let chksum = u32::from_le_bytes(file_buf[off + 8..off + 12].try_into().unwrap());
            if magic.wrapping_add(flags).wrapping_add(chksum) == 0 {
                return 1;
            }
        }
        off += 4;
    }

    0
}

/// Track pages already allocated for kernel segments to avoid double-allocation
/// when adjacent ELF segments share pages.
static mut ALLOC_PAGES: [(u64, usize); 32] = [(0, 0); 32];
static mut ALLOC_COUNT: usize = 0;

/// Check if the page range [base_page .. base_page + num_pages) is already allocated.
/// Returns true if fully covered (no allocation needed).
unsafe fn pages_already_allocated(base_page: u64, num_pages: usize) -> bool {
    for i in 0..ALLOC_COUNT {
        let (alloc_base, alloc_pages) = ALLOC_PAGES[i];
        let alloc_end = alloc_base + (alloc_pages as u64) * 4096;
        if base_page >= alloc_base && base_page + (num_pages as u64) * 4096 <= alloc_end {
            return true;
        }
    }
    false
}

/// Record that pages [base .. base + num_pages * 4096) have been allocated.
unsafe fn record_allocation(base: u64, num_pages: usize) {
    if ALLOC_COUNT < 32 {
        ALLOC_PAGES[ALLOC_COUNT] = (base, num_pages);
        ALLOC_COUNT += 1;
    }
}

/// Load a PT_LOAD segment: allocate pages at physical address and copy data.
/// Handles non-page-aligned paddr and overlapping segments.
fn load_segment(paddr: u64, memsz: u64, file_buf: &[u8], offset: u64, filesz: u64) {
    let page_base = paddr & !0xFFF;
    let page_offset = (paddr - page_base) as usize;
    let total_size = page_offset as u64 + memsz;
    let num_pages = ((total_size + 4095) / 4096) as usize;

    unsafe {
        if !pages_already_allocated(page_base, num_pages) {
            boot::allocate_pages(
                boot::AllocateType::Address(page_base),
                MemoryType::LOADER_DATA,
                num_pages,
            )
            .expect("Failed to allocate pages for segment");
            record_allocation(page_base, num_pages);
        }

        let dest = paddr as *mut u8;
        ptr::copy_nonoverlapping(
            file_buf.as_ptr().add(offset as usize),
            dest,
            filesz as usize,
        );
        if memsz > filesz {
            ptr::write_bytes(
                dest.add(filesz as usize),
                0,
                (memsz - filesz) as usize,
            );
        }
    }
}

/// Load kernel.bin from the boot volume.
pub fn load_kernel() -> KernelInfo {
    println!("[+] Loading kernel...");

    // Open boot volume
    let mut fs = boot::get_image_file_system(boot::image_handle())
        .expect("Failed to open file system");

    let mut root = fs.open_volume().expect("Failed to open volume");

    let kernel_handle = root
        .open(
            cstr16!("\\kernel.bin"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .expect("kernel.bin not found");

    let mut kernel_file = kernel_handle
        .into_regular_file()
        .expect("kernel.bin is not a regular file");

    println!("[+] Found kernel.bin.");

    // Get file size
    let info = kernel_file
        .get_boxed_info::<FileInfo>()
        .expect("Failed to get file info");
    let file_size = info.file_size() as usize;

    println!("[+] Reading kernel into memory...");

    // Read entire file
    let mut file_buf = alloc::vec![0u8; file_size];
    kernel_file
        .read(&mut file_buf)
        .expect("Failed to read kernel");

    // Check ELF magic
    assert!(
        file_size >= EI_NIDENT
            && file_buf[0] == 0x7F
            && file_buf[1] == b'E'
            && file_buf[2] == b'L'
            && file_buf[3] == b'F',
        "Not a valid ELF file"
    );

    // Detect multiboot version
    let mb_version = detect_multiboot_version(&file_buf);
    match mb_version {
        1 => println!("[+] Detected Multiboot1 header."),
        2 => println!("[+] Detected Multiboot2 header."),
        _ => println!("[!] No multiboot header found (proceeding anyway)."),
    }

    let elf_class = file_buf[4];

    let entry_point: u64;

    if elf_class == ELFCLASS32 {
        assert!(
            file_size >= core::mem::size_of::<Elf32Ehdr>(),
            "ELF32 header truncated"
        );
        let ehdr: Elf32Ehdr =
            unsafe { ptr::read_unaligned(file_buf.as_ptr() as *const Elf32Ehdr) };

        assert!(
            ehdr.e_type == ET_EXEC && ehdr.e_machine == EM_386,
            "Not an i386 ELF executable"
        );

        println!("[+] Valid ELF32 i386 executable.");

        let phoff = ehdr.e_phoff as usize;
        let phnum = ehdr.e_phnum as usize;

        for i in 0..phnum {
            let off = phoff + i * core::mem::size_of::<Elf32Phdr>();
            let phdr: Elf32Phdr =
                unsafe { ptr::read_unaligned(file_buf.as_ptr().add(off) as *const Elf32Phdr) };

            if phdr.p_type != PT_LOAD {
                continue;
            }

            load_segment(
                phdr.p_paddr as u64,
                phdr.p_memsz as u64,
                &file_buf,
                phdr.p_offset as u64,
                phdr.p_filesz as u64,
            );
        }

        entry_point = ehdr.e_entry as u64;
    } else if elf_class == ELFCLASS64 {
        assert!(
            file_size >= core::mem::size_of::<Elf64Ehdr>(),
            "ELF64 header truncated"
        );
        let ehdr: Elf64Ehdr =
            unsafe { ptr::read_unaligned(file_buf.as_ptr() as *const Elf64Ehdr) };

        assert!(
            ehdr.e_type == ET_EXEC && ehdr.e_machine == EM_X86_64,
            "Not an x86_64 ELF executable"
        );

        println!("[+] Valid ELF64 x86_64 executable.");

        let phoff = ehdr.e_phoff as usize;
        let phnum = ehdr.e_phnum as usize;

        for i in 0..phnum {
            let off = phoff + i * core::mem::size_of::<Elf64Phdr>();
            let phdr: Elf64Phdr =
                unsafe { ptr::read_unaligned(file_buf.as_ptr().add(off) as *const Elf64Phdr) };

            if phdr.p_type != PT_LOAD {
                continue;
            }

            load_segment(
                phdr.p_paddr,
                phdr.p_memsz,
                &file_buf,
                phdr.p_offset,
                phdr.p_filesz,
            );
        }

        entry_point = ehdr.e_entry;
    } else {
        panic!("Unsupported ELF class: {}", elf_class);
    }

    println!("[+] Kernel loaded successfully.");

    let boot_mode = if mb_version > 0 {
        BootMode::Protected32
    } else if elf_class == ELFCLASS64 {
        BootMode::Long64
    } else {
        // 32-bit ELF without multiboot — still use trampoline, but no MBI
        BootMode::Protected32
    };

    KernelInfo {
        entry_point,
        mb_version,
        boot_mode,
    }
}
