#include "efi-local.h"

EFI_STATUS EFIAPI
efi_init(EFI_SYSTEM_TABLE *systable) {
    ST = systable;
    BS = systable->BootServices;

    return EFI_SUCCESS;
}

EFI_STATUS EFIAPI
efi_is_os_present(void) {
    return ST->RuntimeServices->GetVariable(L"OsIndications", &gEfiGlobalVariableGuid, NULL, NULL, NULL);
}

EFI_STATUS EFIAPI
efi_get_memory_map(
    EFI_MEMORY_DESCRIPTOR **map,
    UINTN *map_size,
    UINTN *map_key,
    UINTN *desc_size,
    UINT32 *desc_version)
{
    EFI_STATUS status;

    /* First call: get required size */
    *map_size = 0;
    status = BS->GetMemoryMap(map_size, NULL, map_key, desc_size, desc_version);
    /* Expected EFI_BUFFER_TOO_SMALL */

    /* Add extra space — the allocation itself may create new entries */
    *map_size += 2 * (*desc_size);

    status = BS->AllocatePool(EfiLoaderData, *map_size, (void **)map);
    if (EFI_ERROR(status)) return status;

    status = BS->GetMemoryMap(map_size, *map, map_key, desc_size, desc_version);
    if (EFI_ERROR(status)) {
        BS->FreePool(*map);
        return status;
    }

    return EFI_SUCCESS;
}

EFI_STATUS EFIAPI
efi_exit_boot_services(EFI_HANDLE image, UINTN mapkey) {
    return ST->BootServices->ExitBootServices(image, mapkey);
}

static UINT32
efi_memtype_to_multiboot(UINT32 efi_type) {
    switch (efi_type) {
    case EfiConventionalMemory:
    case EfiLoaderCode:
    case EfiLoaderData:
    case EfiBootServicesCode:
    case EfiBootServicesData:
        return 1; /* available */
    case EfiACPIReclaimMemory:
        return 3; /* ACPI reclaimable */
    case EfiACPIMemoryNVS:
        return 4; /* ACPI NVS */
    default:
        return 2; /* reserved */
    }
}

void
efi_build_multiboot_info(
    multiboot_info *mbi,
    multiboot_mmap_entry *mmap_buf,
    UINTN mmap_buf_sz,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size)
{
    /* Zero out the struct */
    UINT8 *p = (UINT8 *)mbi;
    for (UINTN i = 0; i < sizeof(multiboot_info); i++)
        p[i] = 0;

    UINT32 mem_lower = 0;  /* KB below 1MB */
    UINT32 mem_upper = 0;  /* KB above 1MB */

    UINTN num_entries = efi_map_size / desc_size;
    UINTN max_mmap = mmap_buf_sz / sizeof(multiboot_mmap_entry);
    UINTN mmap_idx = 0;

    UINT8 *desc_ptr = (UINT8 *)efi_map;

    for (UINTN i = 0; i < num_entries && mmap_idx < max_mmap; i++) {
        EFI_MEMORY_DESCRIPTOR *desc = (EFI_MEMORY_DESCRIPTOR *)desc_ptr;
        UINT64 base = desc->PhysicalStart;
        UINT64 size = desc->NumberOfPages * 4096ULL;
        UINT32 mb_type = efi_memtype_to_multiboot(desc->Type);

        /* Compute mem_lower / mem_upper for available memory */
        if (mb_type == 1) {
            if (base < 0x100000) {
                UINT64 end = base + size;
                if (end > 0x100000) end = 0x100000;
                UINT32 kb = (UINT32)((end - base) / 1024);
                mem_lower += kb;
            }
            if (base + size > 0x100000) {
                UINT64 start = base;
                if (start < 0x100000) start = 0x100000;
                UINT32 kb = (UINT32)((base + size - start) / 1024);
                mem_upper += kb;
            }
        }

        /* Build multiboot mmap entry */
        mmap_buf[mmap_idx].size = sizeof(multiboot_mmap_entry) - 4;
        mmap_buf[mmap_idx].addr = base;
        mmap_buf[mmap_idx].len  = size;
        mmap_buf[mmap_idx].type = mb_type;
        mmap_idx++;

        desc_ptr += desc_size;
    }

    mbi->flags     = MB_INFO_MEMORY | MB_INFO_MEM_MAP;
    mbi->mem_lower = mem_lower;
    mbi->mem_upper = mem_upper;
    mbi->mmap_length = (UINT32)(mmap_idx * sizeof(multiboot_mmap_entry));
    mbi->mmap_addr   = (UINT32)(UINTN)mmap_buf;
}
