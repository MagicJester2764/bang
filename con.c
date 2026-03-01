#include "con.h"

EFI_STATUS EFIAPI 
con_print(CHAR16 *str) {
    return ST->ConOut->OutputString(ST->ConOut, str);
}

EFI_STATUS EFIAPI
con_reset(void) {
    return ST->ConIn->Reset(ST->ConIn, FALSE);
}

EFI_STATUS EFIAPI
con_read_key(EFI_INPUT_KEY *key) {
    return ST->ConIn->ReadKeyStroke(ST->ConIn, key);
}

EFI_STATUS EFIAPI
con_pause(void) {
    EFI_STATUS status;
    EFI_INPUT_KEY key;

    status = con_print(L"Press any key to continue...\r\n");
    if (EFI_ERROR(status)) return status;

    while ((status = con_read_key(&key)) == EFI_NOT_READY);
    
    return status;
}
