use alloc::vec::Vec;
use uefi::boot;
use uefi::mem::memory_map::MemoryType;
use uefi::println;
use uefi::proto::media::file::{Directory, File, FileAttribute, FileMode, FileType};
use uefi::Char16;

/// Information about a loaded boot module.
pub struct ModuleInfo {
    /// ASCII name of the module (null-terminated, stored in static buffer).
    pub name_ptr: u32,
    /// Physical start address of the module data.
    pub phys_start: u64,
    /// Size in bytes.
    pub size: u64,
}

/// Static buffer for module name strings — must survive ExitBootServices.
/// Layout: packed null-terminated ASCII strings, one after another.
static mut MOD_NAMES: [u8; 4096] = [0u8; 4096];
static mut MOD_NAMES_POS: usize = 0;

/// Store a module name in the static buffer and return its physical address.
///
/// # Safety
/// Must be called before `exit_boot_services`.
unsafe fn store_name(name: &[Char16]) -> u32 {
    let pos = MOD_NAMES_POS;
    let buf = &raw mut MOD_NAMES;

    // Convert UCS-2 to ASCII, skipping non-ASCII chars
    let mut written = 0;
    for &ch in name {
        let val: u16 = ch.into();
        if val == 0 {
            break;
        }
        if pos + written >= 4095 {
            break;
        }
        (*buf)[pos + written] = if val < 128 { val as u8 } else { b'?' };
        written += 1;
    }
    (*buf)[pos + written] = 0; // null terminator
    MOD_NAMES_POS = pos + written + 1;

    (*buf).as_ptr().add(pos) as u32
}

/// Load all files from the `\drivers\` directory on the boot volume.
///
/// Must be called before `exit_boot_services` (needs UEFI filesystem access).
/// Returns a vector of loaded module descriptors.
pub fn load_modules() -> Vec<ModuleInfo> {
    let mut modules = Vec::new();

    let mut fs = match boot::get_image_file_system(boot::image_handle()) {
        Ok(fs) => fs,
        Err(_) => {
            println!("[!] Could not open file system for modules");
            return modules;
        }
    };

    let mut root = fs.open_volume().expect("Failed to open volume");

    // Try to open \drivers\ directory
    let dir_handle = match root.open(
        cstr16!("\\drivers"),
        FileMode::Read,
        FileAttribute::empty(),
    ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("[*] No \\drivers directory found, skipping module loading");
            return modules;
        }
    };

    let mut dir = match dir_handle.into_type().expect("Failed to get file type") {
        FileType::Dir(d) => d,
        FileType::Regular(_) => {
            println!("[!] \\drivers is not a directory");
            return modules;
        }
    };

    println!("[+] Scanning \\drivers\\ for modules...");

    // Read directory entries
    // We need a buffer for read_entry — use a generous stack buffer
    let mut entry_buf = [0u8; 512];

    loop {
        let entry = match dir.read_entry(&mut entry_buf) {
            Ok(Some(info)) => info,
            Ok(None) => break, // no more entries
            Err(_) => break,
        };

        // Skip '.' and '..' entries, and directories
        if entry.is_directory() {
            continue;
        }

        let file_name = entry.file_name();
        let file_size = entry.file_size() as usize;

        if file_size == 0 {
            continue;
        }

        // Open the file from the drivers directory
        let file_handle = match open_file_in_dir(&mut dir, file_name) {
            Some(h) => h,
            None => continue,
        };

        let mut regular_file = match file_handle.into_regular_file() {
            Some(f) => f,
            None => continue,
        };

        // Allocate pages for the module
        let num_pages = (file_size + 4095) / 4096;
        let phys_addr = boot::allocate_pages(
            boot::AllocateType::AnyPages,
            MemoryType::LOADER_DATA,
            num_pages,
        )
        .expect("Failed to allocate pages for module");

        // Read file contents into allocated pages
        let buf =
            unsafe { core::slice::from_raw_parts_mut(phys_addr.as_ptr(), num_pages * 4096) };
        regular_file.read(buf).expect("Failed to read module file");

        // Store name in static buffer
        let name_ptr = unsafe { store_name(file_name.as_slice_with_nul()) };

        // Print what we loaded (convert name for display)
        print_module_name(file_name.as_slice_with_nul(), phys_addr.as_ptr() as u64, file_size);

        modules.push(ModuleInfo {
            name_ptr,
            phys_start: phys_addr.as_ptr() as u64,
            size: file_size as u64,
        });
    }

    println!("[+] Loaded {} module(s)", modules.len());
    modules
}

/// Open a file by CStr16 name inside an already-open directory.
fn open_file_in_dir(
    dir: &mut Directory,
    name: &uefi::CStr16,
) -> Option<uefi::proto::media::file::FileHandle> {
    dir.open(name, FileMode::Read, FileAttribute::empty()).ok()
}

/// Print module load info.
fn print_module_name(_name: &[Char16], addr: u64, size: usize) {
    // uefi println doesn't support &[u8] easily, just print address info
    println!(
        "[+] Loaded module: ({} bytes @ {:#x})",
        size, addr
    );
}
