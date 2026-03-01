#ifndef _FS_H_
#define _FS_H_

#include <efi.h>
#include <efilib.h>

EFI_STATUS EFIAPI fs_open_volume(EFI_HANDLE image, EFI_FILE_HANDLE *root);

EFI_STATUS EFIAPI fs_find_kernel(EFI_HANDLE image);

#endif /* _FS_H_ */
