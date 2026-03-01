#ifndef _EFI_LOCAL_H_
#define _EFI_LOCAL_H_

#include <efi.h>
#include <efilib.h>

EFI_STATUS EFIAPI efi_init(EFI_SYSTEM_TABLE *systable);

EFI_STATUS EFIAPI efi_is_os_present(void);

EFI_STATUS EFIAPI efi_exit_boot_services(EFI_HANDLE image, UINTN mapkey);

#endif /* _EFI_LOCAL_H_ */