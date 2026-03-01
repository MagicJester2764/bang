#include "fs.h"
#include "con.h"

EFI_STATUS EFIAPI
fs_open_volume(EFI_HANDLE image, EFI_FILE_HANDLE *root) {
    EFI_STATUS status;
    EFI_LOADED_IMAGE *loaded_image;
    EFI_SIMPLE_FILE_SYSTEM_PROTOCOL *fs;

    status = BS->HandleProtocol(image, &gEfiLoadedImageProtocolGuid,
                                (void **)&loaded_image);
    if (EFI_ERROR(status)) return status;

    status = BS->HandleProtocol(loaded_image->DeviceHandle,
                                &gEfiSimpleFileSystemProtocolGuid,
                                (void **)&fs);
    if (EFI_ERROR(status)) return status;

    status = fs->OpenVolume(fs, root);
    return status;
}

EFI_STATUS EFIAPI
fs_find_kernel(EFI_HANDLE image) {
    EFI_STATUS status;
    EFI_FILE_HANDLE root;
    EFI_FILE_HANDLE kernel_file;

    status = fs_open_volume(image, &root);
    if (EFI_ERROR(status)) return status;

    status = root->Open(root, &kernel_file, L"\\kernel.bin",
                        EFI_FILE_MODE_READ, 0);
    if (EFI_ERROR(status)) {
        con_print(L"[-] kernel.bin not found.\r\n");
        root->Close(root);
        return EFI_SUCCESS;
    }

    con_print(L"[+] Found kernel.bin.\r\n");
    kernel_file->Close(kernel_file);
    root->Close(root);
    return EFI_SUCCESS;
}
