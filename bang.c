#include "con.h"
#include "efi-local.h"

EFI_STATUS EFIAPI
efi_main(EFI_HANDLE image, EFI_SYSTEM_TABLE *systable) {
    EFI_STATUS status;
    EFI_INPUT_KEY key;
    UINTN map_size = 0, map_key, desc_size;
    UINT32 desc_version;
    EFI_MEMORY_DESCRIPTOR *map = NULL;

    efi_init(systable);

    status = con_print(L"=======================\r\n");
    if (EFI_ERROR(status)) return status;

    status = con_print(L"===== Bang v0.1.0 =====\r\n");
    if (EFI_ERROR(status)) return status;

    status = con_print(L"=======================\r\n\r\n");
    if (EFI_ERROR(status)) return status;

    status = con_print(L"[+] Grabbing memory map...\r\n");
    if (EFI_ERROR(status)) return status;

    // status = BS->GetMemoryMap(&map_size, map, &map_key, &desc_size, &desc_version);
    // if (EFI_ERROR(status)) return status;

    status = con_print(L"[+] Searching for EFI applications...\r\n");
    if (EFI_ERROR(status)) return status;

    status = con_print(L"[+] Found 0 applications.\r\n\r\n");
    if (EFI_ERROR(status)) return status;

    status = con_reset();
    if (EFI_ERROR(status)) return status;

    status = con_pause();
    if (EFI_ERROR(status)) return status;

    status = con_print(L"Exiting boot services...\r\n");
    if (EFI_ERROR(status)) return status;

    status = efi_exit_boot_services(image, 0);
    if (EFI_ERROR(status)) return status;

    status = con_print(L"Boot services exited successfully.\r\n");
    if (EFI_ERROR(status)) return status;

    return status;
}

