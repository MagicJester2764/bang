#ifndef _EFI_LOCAL_H_
#define _EFI_LOCAL_H_

#include <efi.h>
#include <efilib.h>

/* Multiboot1 constants */
#define MB_INFO_MEMORY   0x00000001
#define MB_INFO_MEM_MAP  0x00000040

/* Multiboot1 memory map entry (as passed in mmap_addr buffer) */
#pragma pack(1)
typedef struct {
    UINT32 size;       /* size of this entry minus 4 */
    UINT64 base_addr;
    UINT64 length;
    UINT32 type;       /* 1 = available, 2 = reserved, 3 = ACPI, 4 = NVS, 5 = bad */
} multiboot_mmap_entry;

/* Multiboot1 info structure (subset — memory + mmap fields) */
typedef struct {
    UINT32 flags;
    UINT32 mem_lower;
    UINT32 mem_upper;
    UINT32 boot_device;
    UINT32 cmdline;
    UINT32 mods_count;
    UINT32 mods_addr;
    UINT32 syms[4];
    UINT32 mmap_length;
    UINT32 mmap_addr;
} multiboot_info;
#pragma pack()

/* Multiboot2 constants */
#define MB2_TAG_TYPE_END   0
#define MB2_TAG_TYPE_MMAP  6

/* Multiboot2 memory map entry */
#pragma pack(1)
typedef struct {
    UINT64 base_addr;
    UINT64 length;
    UINT32 type;     /* 1 = available, 2 = reserved, 3 = ACPI, 4 = NVS, 5 = bad */
    UINT32 reserved;
} mb2_mmap_entry;

/* Multiboot2 tag header (common to all tags) */
typedef struct {
    UINT32 type;
    UINT32 size;
} mb2_tag_header;
#pragma pack()

EFI_STATUS EFIAPI efi_init(EFI_SYSTEM_TABLE *systable);

EFI_STATUS EFIAPI efi_is_os_present(void);

/*
 * efi_get_memory_map - Retrieve the EFI memory map.
 *
 * map:          receives pointer to allocated memory map buffer
 * map_size:     receives the size of the memory map
 * map_key:      receives the map key (needed for ExitBootServices)
 * desc_size:    receives the size of each descriptor
 * desc_version: receives the descriptor version
 */
EFI_STATUS EFIAPI efi_get_memory_map(
    EFI_MEMORY_DESCRIPTOR **map,
    UINTN *map_size,
    UINTN *map_key,
    UINTN *desc_size,
    UINT32 *desc_version
);

EFI_STATUS EFIAPI efi_exit_boot_services(EFI_HANDLE image, UINTN mapkey);

/*
 * efi_build_multiboot_info - Build Multiboot1 boot information from the
 * EFI memory map into caller-provided structures.
 *
 * mbi:          pointer to multiboot_info struct to fill
 * mmap_entries: array of multiboot_mmap_entry to fill
 * max_entries:  max number of mmap entries
 * efi_map:      EFI memory map
 * efi_map_size: size of EFI memory map
 * desc_size:    size of each EFI memory descriptor
 */
void efi_build_multiboot_info(
    multiboot_info *mbi,
    multiboot_mmap_entry *mmap_entries,
    UINTN max_entries,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size
);

/*
 * efi_build_multiboot2_info - Build Multiboot2 boot information from the
 * EFI memory map into a caller-provided buffer.
 *
 * buf:          output buffer (must be 8-byte aligned)
 * buf_size:     size of buf in bytes
 * efi_map:      EFI memory map
 * efi_map_size: size of EFI memory map
 * desc_size:    size of each EFI memory descriptor
 *
 * Returns:      total size written to buf (0 on error)
 */
UINT32 efi_build_multiboot2_info(
    void *buf,
    UINTN buf_size,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size
);

#endif /* _EFI_LOCAL_H_ */
