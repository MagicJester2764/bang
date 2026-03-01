#ifndef _EFI_LOCAL_H_
#define _EFI_LOCAL_H_

#include <efi.h>
#include <efilib.h>

/* Multiboot info flags */
#define MB_INFO_MEMORY   0x001
#define MB_INFO_MEM_MAP  0x040

/* Multiboot memory map entry (matches multiboot spec) */
#pragma pack(1)
typedef struct {
    UINT32 size;     /* size of this entry minus 4 */
    UINT64 addr;
    UINT64 len;
    UINT32 type;     /* 1 = available, 2 = reserved, ... */
} multiboot_mmap_entry;

typedef struct {
    UINT32 flags;
    UINT32 mem_lower;     /* KB of lower memory (below 1MB) */
    UINT32 mem_upper;     /* KB of upper memory (above 1MB) */
    UINT32 boot_device;
    UINT32 cmdline;
    UINT32 mods_count;
    UINT32 mods_addr;
    UINT32 syms[4];
    UINT32 mmap_length;
    UINT32 mmap_addr;
    UINT32 drives_length;
    UINT32 drives_addr;
    UINT32 config_table;
    UINT32 boot_loader_name;
    UINT32 apm_table;
    UINT32 vbe_control_info;
    UINT32 vbe_mode_info;
    UINT16 vbe_mode;
    UINT16 vbe_interface_seg;
    UINT16 vbe_interface_off;
    UINT16 vbe_interface_len;
} multiboot_info;
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
 * efi_build_multiboot_info - Build a multiboot info struct from the EFI
 * memory map. The mbi and mmap_buf must be allocated by the caller in
 * memory that will persist after ExitBootServices.
 *
 * mbi:          pointer to multiboot_info to populate
 * mmap_buf:     buffer for multiboot mmap entries
 * mmap_buf_sz:  size of mmap_buf in bytes
 * efi_map:      EFI memory map
 * efi_map_size: size of EFI memory map
 * desc_size:    size of each EFI memory descriptor
 */
void efi_build_multiboot_info(
    multiboot_info *mbi,
    multiboot_mmap_entry *mmap_buf,
    UINTN mmap_buf_sz,
    EFI_MEMORY_DESCRIPTOR *efi_map,
    UINTN efi_map_size,
    UINTN desc_size
);

#endif /* _EFI_LOCAL_H_ */
