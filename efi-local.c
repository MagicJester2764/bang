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
efi_exit_boot_services(EFI_HANDLE image, UINTN mapkey) {
    return ST->BootServices->ExitBootServices(image, mapkey);
}
