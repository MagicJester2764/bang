#include "fs.h"
#include "con.h"

/* ELF32 types and structures */
#define EI_NIDENT 16
#define ET_EXEC   2
#define EM_386    3
#define PT_LOAD   1

#define ELFMAG0 0x7f
#define ELFMAG1 'E'
#define ELFMAG2 'L'
#define ELFMAG3 'F'

#pragma pack(1)
typedef struct {
    UINT8  e_ident[EI_NIDENT];
    UINT16 e_type;
    UINT16 e_machine;
    UINT32 e_version;
    UINT32 e_entry;
    UINT32 e_phoff;
    UINT32 e_shoff;
    UINT32 e_flags;
    UINT16 e_ehsize;
    UINT16 e_phentsize;
    UINT16 e_phnum;
    UINT16 e_shentsize;
    UINT16 e_shnum;
    UINT16 e_shstrndx;
} Elf32_Ehdr;

typedef struct {
    UINT32 p_type;
    UINT32 p_offset;
    UINT32 p_vaddr;
    UINT32 p_paddr;
    UINT32 p_filesz;
    UINT32 p_memsz;
    UINT32 p_flags;
    UINT32 p_align;
} Elf32_Phdr;
#pragma pack()

static void
mem_set(void *dst, UINT8 val, UINTN count) {
    UINT8 *d = (UINT8 *)dst;
    for (UINTN i = 0; i < count; i++)
        d[i] = val;
}

static void
mem_copy(void *dst, const void *src, UINTN count) {
    UINT8 *d = (UINT8 *)dst;
    const UINT8 *s = (const UINT8 *)src;
    for (UINTN i = 0; i < count; i++)
        d[i] = s[i];
}

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
fs_load_kernel(EFI_HANDLE image, UINT32 *entry_point) {
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
        return status;
    }

    con_print(L"[+] Found kernel.bin.\r\n");

    /* Get file size via EFI_FILE_INFO */
    UINTN info_size = 0;
    status = kernel_file->GetInfo(kernel_file, &gEfiFileInfoGuid,
                                  &info_size, NULL);
    /* Expected EFI_BUFFER_TOO_SMALL — info_size now holds required size */

    UINT8 *info_buf;
    status = BS->AllocatePool(EfiLoaderData, info_size, (void **)&info_buf);
    if (EFI_ERROR(status)) goto close_files;

    status = kernel_file->GetInfo(kernel_file, &gEfiFileInfoGuid,
                                  &info_size, info_buf);
    if (EFI_ERROR(status)) {
        BS->FreePool(info_buf);
        goto close_files;
    }

    EFI_FILE_INFO *file_info = (EFI_FILE_INFO *)info_buf;
    UINTN file_size = (UINTN)file_info->FileSize;
    BS->FreePool(info_buf);

    con_print(L"[+] Reading kernel into memory...\r\n");

    /* Allocate buffer and read entire file */
    UINT8 *file_buf;
    status = BS->AllocatePool(EfiLoaderData, file_size, (void **)&file_buf);
    if (EFI_ERROR(status)) goto close_files;

    UINTN read_size = file_size;
    status = kernel_file->Read(kernel_file, &read_size, file_buf);
    if (EFI_ERROR(status)) {
        BS->FreePool(file_buf);
        goto close_files;
    }

    /* Parse ELF32 header */
    Elf32_Ehdr *ehdr = (Elf32_Ehdr *)file_buf;

    if (ehdr->e_ident[0] != ELFMAG0 || ehdr->e_ident[1] != ELFMAG1 ||
        ehdr->e_ident[2] != ELFMAG2 || ehdr->e_ident[3] != ELFMAG3) {
        con_print(L"[-] Not a valid ELF file.\r\n");
        BS->FreePool(file_buf);
        status = EFI_LOAD_ERROR;
        goto close_files;
    }

    if (ehdr->e_type != ET_EXEC || ehdr->e_machine != EM_386) {
        con_print(L"[-] Not an i386 ELF executable.\r\n");
        BS->FreePool(file_buf);
        status = EFI_LOAD_ERROR;
        goto close_files;
    }

    con_print(L"[+] Valid ELF32 i386 executable.\r\n");

    /* Load PT_LOAD segments */
    Elf32_Phdr *phdr = (Elf32_Phdr *)(file_buf + ehdr->e_phoff);

    for (UINT16 i = 0; i < ehdr->e_phnum; i++) {
        if (phdr[i].p_type != PT_LOAD)
            continue;

        /* Allocate pages at the segment's physical address */
        UINTN num_pages = (phdr[i].p_memsz + 4095) / 4096;
        EFI_PHYSICAL_ADDRESS seg_addr = phdr[i].p_paddr;

        status = BS->AllocatePages(AllocateAddress, EfiLoaderData,
                                    num_pages, &seg_addr);
        if (EFI_ERROR(status)) {
            con_print(L"[-] Failed to allocate pages for segment.\r\n");
            BS->FreePool(file_buf);
            goto close_files;
        }

        /* Copy segment data */
        mem_copy((void *)(UINTN)seg_addr,
                 file_buf + phdr[i].p_offset,
                 phdr[i].p_filesz);

        /* Zero BSS (memsz > filesz) */
        if (phdr[i].p_memsz > phdr[i].p_filesz) {
            mem_set((void *)(UINTN)(seg_addr + phdr[i].p_filesz),
                    0,
                    phdr[i].p_memsz - phdr[i].p_filesz);
        }
    }

    *entry_point = ehdr->e_entry;
    con_print(L"[+] Kernel loaded successfully.\r\n");

    BS->FreePool(file_buf);

close_files:
    kernel_file->Close(kernel_file);
    root->Close(root);
    return status;
}
