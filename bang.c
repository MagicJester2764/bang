#include "con.h"
#include "efi-local.h"
#include "fs.h"

/* Assembly trampoline: drops to 32-bit protected mode and jumps to kernel */
extern void boot_jump_to_kernel(UINT32 entry, UINT32 mbi_addr);

/* Static storage for multiboot info — must survive ExitBootServices */
static multiboot_info mbi;
static multiboot_mmap_entry mmap_entries[256];

EFI_STATUS EFIAPI
efi_main(EFI_HANDLE image, EFI_SYSTEM_TABLE *systable) {
    EFI_STATUS status;
    UINT32 entry_point;
    EFI_MEMORY_DESCRIPTOR *efi_map;
    UINTN map_size, map_key, desc_size;
    UINT32 desc_version;

    efi_init(systable);

    con_print(L"=======================\r\n");
    con_print(L"===== Bang v0.1.0 =====\r\n");
    con_print(L"=======================\r\n\r\n");

    /* Load kernel ELF into memory */
    con_print(L"[+] Loading kernel...\r\n");
    status = fs_load_kernel(image, &entry_point);
    if (EFI_ERROR(status)) {
        con_print(L"[-] Failed to load kernel.\r\n");
        return status;
    }

    /* Get memory map (must be done right before ExitBootServices) */
    con_print(L"[+] Getting memory map...\r\n");
    status = efi_get_memory_map(&efi_map, &map_size, &map_key,
                                 &desc_size, &desc_version);
    if (EFI_ERROR(status)) {
        con_print(L"[-] Failed to get memory map.\r\n");
        return status;
    }

    /* Build multiboot info from EFI memory map */
    efi_build_multiboot_info(&mbi, mmap_entries, sizeof(mmap_entries),
                              efi_map, map_size, desc_size);

    /* Exit boot services — no more EFI calls after this!
     * We must NOT call con_print or any BS function past this point.
     * If ExitBootServices fails, the map key changed — retry once. */
    status = efi_exit_boot_services(image, map_key);
    if (EFI_ERROR(status)) {
        /* Map key was stale — get a fresh map and retry */
        map_size = 0;
        status = efi_get_memory_map(&efi_map, &map_size, &map_key,
                                     &desc_size, &desc_version);
        if (EFI_ERROR(status))
            return status;

        efi_build_multiboot_info(&mbi, mmap_entries, sizeof(mmap_entries),
                                  efi_map, map_size, desc_size);

        status = efi_exit_boot_services(image, map_key);
        if (EFI_ERROR(status))
            return status;
    }

    /* Jump to kernel — does not return */
    boot_jump_to_kernel(entry_point, (UINT32)(UINTN)&mbi);

    /* Should never reach here */
    for (;;) __asm__ volatile("hlt");
    return EFI_SUCCESS;
}
