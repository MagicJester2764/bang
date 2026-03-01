#ifndef _FS_H_
#define _FS_H_

#include <efi.h>
#include <efilib.h>

EFI_STATUS EFIAPI fs_open_volume(EFI_HANDLE image, EFI_FILE_HANDLE *root);

/*
 * fs_load_kernel - Load kernel.bin from the boot volume, parse its ELF32
 * headers, load PT_LOAD segments to their physical addresses, and return
 * the entry point.
 *
 * image:       EFI image handle
 * entry_point: receives the ELF entry point address (e_entry)
 */
EFI_STATUS EFIAPI fs_load_kernel(EFI_HANDLE image, UINT32 *entry_point);

#endif /* _FS_H_ */
