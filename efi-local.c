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

EFI_STATUS EFIAPI
efi_query_gop(fb_info *fb) {
    EFI_STATUS status;
    EFI_GRAPHICS_OUTPUT_PROTOCOL *gop;

    status = BS->LocateProtocol(&gEfiGraphicsOutputProtocolGuid, NULL,
                                (void **)&gop);
    if (EFI_ERROR(status))
        return status;

    EFI_GRAPHICS_OUTPUT_MODE_INFORMATION *info = gop->Mode->Info;

    fb->addr   = gop->Mode->FrameBufferBase;
    fb->width  = info->HorizontalResolution;
    fb->height = info->VerticalResolution;

    switch (info->PixelFormat) {
    case PixelRedGreenBlueReserved8BitPerColor:
        fb->bpp       = 32;
        fb->type      = 1;
        fb->red_pos   = 0;
        fb->red_size  = 8;
        fb->green_pos = 8;
        fb->green_size = 8;
        fb->blue_pos  = 16;
        fb->blue_size = 8;
        break;
    case PixelBlueGreenRedReserved8BitPerColor:
        fb->bpp       = 32;
        fb->type      = 1;
        fb->red_pos   = 16;
        fb->red_size  = 8;
        fb->green_pos = 8;
        fb->green_size = 8;
        fb->blue_pos  = 0;
        fb->blue_size = 8;
        break;
    default:
        fb->bpp       = 32;
        fb->type      = 1;
        fb->red_pos   = 0;
        fb->red_size  = 8;
        fb->green_pos = 8;
        fb->green_size = 8;
        fb->blue_pos  = 16;
        fb->blue_size = 8;
        break;
    }

    fb->pitch = info->PixelsPerScanLine * (fb->bpp / 8);

    return EFI_SUCCESS;
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
    UINTN desc_size,
    fb_info *fb)
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

    /* Framebuffer tag (type=8) if GOP info is available */
    if (fb) {
        /*
         * MB2 framebuffer tag layout:
         *   +0x00: u32 type (8)
         *   +0x04: u32 size
         *   +0x08: u64 framebuffer_addr
         *   +0x10: u32 framebuffer_pitch
         *   +0x14: u32 framebuffer_width
         *   +0x18: u32 framebuffer_height
         *   +0x1C: u8  framebuffer_bpp
         *   +0x1D: u8  framebuffer_type (1 = RGB direct color)
         *   +0x1E: u8  reserved
         *   -- color info for type 1 (RGB) --
         *   +0x1F: u8  red_field_position
         *   +0x20: u8  red_mask_size
         *   +0x21: u8  green_field_position
         *   +0x22: u8  green_mask_size
         *   +0x23: u8  blue_field_position
         *   +0x24: u8  blue_mask_size
         *   Total: 0x25 = 37 bytes
         */
        UINTN fb_tag_size = 37;
        if (pos + fb_tag_size <= buf_size) {
            *(UINT32 *)(out + pos + 0x00) = MB2_TAG_TYPE_FRAMEBUFFER;
            *(UINT32 *)(out + pos + 0x04) = (UINT32)fb_tag_size;
            *(UINT64 *)(out + pos + 0x08) = fb->addr;
            *(UINT32 *)(out + pos + 0x10) = fb->pitch;
            *(UINT32 *)(out + pos + 0x14) = fb->width;
            *(UINT32 *)(out + pos + 0x18) = fb->height;
            *(UINT8  *)(out + pos + 0x1C) = fb->bpp;
            *(UINT8  *)(out + pos + 0x1D) = fb->type;
            *(UINT8  *)(out + pos + 0x1E) = 0;
            *(UINT8  *)(out + pos + 0x1F) = fb->red_pos;
            *(UINT8  *)(out + pos + 0x20) = fb->red_size;
            *(UINT8  *)(out + pos + 0x21) = fb->green_pos;
            *(UINT8  *)(out + pos + 0x22) = fb->green_size;
            *(UINT8  *)(out + pos + 0x23) = fb->blue_pos;
            *(UINT8  *)(out + pos + 0x24) = fb->blue_size;
            pos += fb_tag_size;
        }

        /* Align to 8 bytes */
        pos = (pos + 7) & ~(UINTN)7;
    }

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
