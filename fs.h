#ifndef _FS_H_
#define _FS_H_

#include <efi.h>
#include <efilib.h>

EFI_STATUS EFIAPI fs_open_volume(EFI_HANDLE image, EFI_FILE_HANDLE *root);

/*
 * fs_load_kernel - Load kernel.bin from the boot volume, parse its ELF
 * headers (ELF32 or ELF64), load PT_LOAD segments to their physical
 * addresses, detect the multiboot version, and return the entry point
 * (truncated to 32 bits for the trampoline).
 *
 * image:       EFI image handle
 * entry_point: receives the ELF entry point address (e_entry)
 * mb_version:  receives the detected multiboot version (1 or 2)
 */
EFI_STATUS EFIAPI fs_load_kernel(EFI_HANDLE image, UINT32 *entry_point,
                                  UINT32 *mb_version);

#endif /* _FS_H_ */
