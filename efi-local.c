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
efi_memtype_to_mb(UINT32 efi_type) {
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

static void
mb_zero(void *dst, UINTN count) {
    UINT8 *d = (UINT8 *)dst;
    for (UINTN i = 0; i < count; i++)
        d[i] = 0;
}

void
efi_build_multiboot_info(
    multiboot_info *mbi,
    multiboot_mmap_entry *mmap_entries,
    UINTN max_entries,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size)
{
    UINTN num_efi_entries = efi_map_size / desc_size;

    mb_zero(mbi, sizeof(*mbi));

    UINT64 mem_lower = 0;
    UINT64 mem_upper = 0;
    UINTN mmap_count = 0;
    UINT8 *desc_ptr = (UINT8 *)efi_map;

    for (UINTN i = 0; i < num_efi_entries && mmap_count < max_entries; i++) {
        EFI_MEMORY_DESCRIPTOR *desc = (EFI_MEMORY_DESCRIPTOR *)desc_ptr;
        UINT64 base = desc->PhysicalStart;
        UINT64 length = desc->NumberOfPages * 4096ULL;
        UINT32 type = efi_memtype_to_mb(desc->Type);

        multiboot_mmap_entry *ent = &mmap_entries[mmap_count];
        ent->size      = sizeof(multiboot_mmap_entry) - 4;
        ent->base_addr = base;
        ent->length    = length;
        ent->type      = type;
        mmap_count++;

        /* Track conventional memory for mem_lower / mem_upper */
        if (type == 1) {
            if (base < 0x100000)
                mem_lower += length / 1024;
            else if (base == 0x100000 || (base > 0x100000 && mem_upper > 0))
                mem_upper += length / 1024;
        }

        desc_ptr += desc_size;
    }

    /* Cap at 32-bit limits */
    if (mem_lower > 640)
        mem_lower = 640;

    mbi->flags       = MB_INFO_MEMORY | MB_INFO_MEM_MAP;
    mbi->mem_lower   = (UINT32)mem_lower;
    mbi->mem_upper   = (UINT32)mem_upper;
    mbi->mmap_length = (UINT32)(mmap_count * sizeof(multiboot_mmap_entry));
    mbi->mmap_addr   = (UINT32)(UINTN)mmap_entries;
}

static UINT32
efi_memtype_to_mb2(UINT32 efi_type) {
    return efi_memtype_to_mb(efi_type);
}

static void
mb2_zero(void *dst, UINTN count) {
    UINT8 *d = (UINT8 *)dst;
    for (UINTN i = 0; i < count; i++)
        d[i] = 0;
}

UINT32
efi_build_multiboot2_info(
    void *buf,
    UINTN buf_size,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size)
{
    UINTN num_entries = efi_map_size / desc_size;
    UINT8 *out = (UINT8 *)buf;
    UINTN pos = 0;

    mb2_zero(buf, buf_size);

    /* MB2 boot info fixed header: { u32 total_size, u32 reserved } */
    pos = 8;

    /* Memory map tag (type=6) */
    /* Tag header: type(4) + size(4) + entry_size(4) + entry_version(4) = 16 */
    UINTN mmap_tag_start = pos;
    UINT32 entry_size = (UINT32)sizeof(mb2_mmap_entry);

    /* Write tag type */
    *(UINT32 *)(out + pos) = MB2_TAG_TYPE_MMAP;
    pos += 4;
    /* Placeholder for tag size — fill in later */
    UINTN tag_size_offset = pos;
    pos += 4;
    /* entry_size */
    *(UINT32 *)(out + pos) = entry_size;
    pos += 4;
    /* entry_version */
    *(UINT32 *)(out + pos) = 0;
    pos += 4;

    /* Write mmap entries */
    UINT8 *desc_ptr = (UINT8 *)efi_map;
    for (UINTN i = 0; i < num_entries; i++) {
        if (pos + sizeof(mb2_mmap_entry) > buf_size)
            break;

        EFI_MEMORY_DESCRIPTOR *desc = (EFI_MEMORY_DESCRIPTOR *)desc_ptr;
        mb2_mmap_entry *ent = (mb2_mmap_entry *)(out + pos);

        ent->base_addr = desc->PhysicalStart;
        ent->length    = desc->NumberOfPages * 4096ULL;
        ent->type      = efi_memtype_to_mb2(desc->Type);
        ent->reserved  = 0;

        pos += sizeof(mb2_mmap_entry);
        desc_ptr += desc_size;
    }

    /* Fill in mmap tag size */
    *(UINT32 *)(out + tag_size_offset) = (UINT32)(pos - mmap_tag_start);

    /* Align to 8 bytes */
    pos = (pos + 7) & ~(UINTN)7;

    /* Terminating tag (type=0, size=8) */
    if (pos + 8 > buf_size)
        return 0;
    *(UINT32 *)(out + pos) = MB2_TAG_TYPE_END;
    *(UINT32 *)(out + pos + 4) = 8;
    pos += 8;

    /* Fill in the fixed header */
    *(UINT32 *)(out + 0) = (UINT32)pos;  /* total_size */
    *(UINT32 *)(out + 4) = 0;            /* reserved */

    return (UINT32)pos;
}
