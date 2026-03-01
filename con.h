#ifndef _CON_H_
#define _CON_H_

#include <efi.h>
#include <efilib.h>

EFI_STATUS EFIAPI con_print(CHAR16 *str);
EFI_STATUS EFIAPI con_reset(void);
EFI_STATUS EFIAPI con_read_key(EFI_INPUT_KEY *key);
EFI_STATUS EFIAPI con_pause(void);

#endif /* _CON_H_ */