#ifndef _EFI_LOCAL_H_
#define _EFI_LOCAL_H_

#include <efi.h>
#include <efilib.h>

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
