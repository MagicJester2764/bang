#include "fs.h"
#include "con.h"

/* ELF types and structures */
#define EI_NIDENT  16
#define ET_EXEC    2
#define EM_386     3
#define EM_X86_64  62
#define PT_LOAD    1
#define ELFCLASS32 1
#define ELFCLASS64 2

#define ELFMAG0 0x7f
#define ELFMAG1 'E'
#define ELFMAG2 'L'
#define ELFMAG3 'F'

/* Multiboot header magics */
#define MB1_HEADER_MAGIC 0x1BADB002
#define MB2_HEADER_MAGIC 0xE85250D6

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

typedef struct {
    UINT8  e_ident[EI_NIDENT];
    UINT16 e_type;
    UINT16 e_machine;
    UINT32 e_version;
    UINT64 e_entry;
    UINT64 e_phoff;
    UINT64 e_shoff;
    UINT32 e_flags;
    UINT16 e_ehsize;
    UINT16 e_phentsize;
    UINT16 e_phnum;
    UINT16 e_shentsize;
    UINT16 e_shnum;
    UINT16 e_shstrndx;
} Elf64_Ehdr;

typedef struct {
    UINT32 p_type;
    UINT32 p_flags;
    UINT64 p_offset;
    UINT64 p_vaddr;
    UINT64 p_paddr;
    UINT64 p_filesz;
    UINT64 p_memsz;
    UINT64 p_align;
} Elf64_Phdr;
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

/*
 * detect_multiboot_version - Scan raw ELF file for multiboot headers.
 * Returns 2 (MB2), 1 (MB1), or 0 (unknown).
 * MB2 is checked first (preferred).
 */
static UINT32
detect_multiboot_version(const UINT8 *file_buf, UINTN file_size) {
    /* MB2: magic 0xE85250D6 within first 32768 bytes, 8-byte aligned */
    UINTN mb2_limit = file_size < 32768 ? file_size : 32768;
    for (UINTN off = 0; off + 16 <= mb2_limit; off += 8) {
        UINT32 magic = *(const UINT32 *)(file_buf + off);
        if (magic == MB2_HEADER_MAGIC) {
            UINT32 arch   = *(const UINT32 *)(file_buf + off + 4);
            UINT32 hlen   = *(const UINT32 *)(file_buf + off + 8);
            UINT32 chksum = *(const UINT32 *)(file_buf + off + 12);
            if (magic + arch + hlen + chksum == 0)
                return 2;
        }
    }

    /* MB1: magic 0x1BADB002 within first 8192 bytes, 4-byte aligned */
    UINTN mb1_limit = file_size < 8192 ? file_size : 8192;
    for (UINTN off = 0; off + 12 <= mb1_limit; off += 4) {
        UINT32 magic  = *(const UINT32 *)(file_buf + off);
        if (magic == MB1_HEADER_MAGIC) {
            UINT32 flags  = *(const UINT32 *)(file_buf + off + 4);
            UINT32 chksum = *(const UINT32 *)(file_buf + off + 8);
            if (magic + flags + chksum == 0)
                return 1;
        }
    }

    return 0;
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
fs_load_kernel(EFI_HANDLE image, UINT32 *entry_point, UINT32 *mb_version) {
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

    /* Check ELF magic */
    if (file_size < EI_NIDENT ||
        file_buf[0] != ELFMAG0 || file_buf[1] != ELFMAG1 ||
        file_buf[2] != ELFMAG2 || file_buf[3] != ELFMAG3) {
        con_print(L"[-] Not a valid ELF file.\r\n");
        BS->FreePool(file_buf);
        status = EFI_LOAD_ERROR;
        goto close_files;
    }

    /* Detect multiboot version from raw file */
    *mb_version = detect_multiboot_version(file_buf, file_size);
    if (*mb_version == 1)
        con_print(L"[+] Detected Multiboot1 header.\r\n");
    else if (*mb_version == 2)
        con_print(L"[+] Detected Multiboot2 header.\r\n");
    else
        con_print(L"[!] No multiboot header found (proceeding anyway).\r\n");

    /* Branch on ELF class */
    UINT8 elf_class = file_buf[4];

    if (elf_class == ELFCLASS32) {
        Elf32_Ehdr *ehdr = (Elf32_Ehdr *)file_buf;

        if (ehdr->e_type != ET_EXEC || ehdr->e_machine != EM_386) {
            con_print(L"[-] Not an i386 ELF executable.\r\n");
            BS->FreePool(file_buf);
            status = EFI_LOAD_ERROR;
            goto close_files;
        }

        con_print(L"[+] Valid ELF32 i386 executable.\r\n");

        Elf32_Phdr *phdr = (Elf32_Phdr *)(file_buf + ehdr->e_phoff);

        for (UINT16 i = 0; i < ehdr->e_phnum; i++) {
            if (phdr[i].p_type != PT_LOAD)
                continue;

            UINTN num_pages = (UINTN)((phdr[i].p_memsz + 4095) / 4096);
            EFI_PHYSICAL_ADDRESS seg_addr = phdr[i].p_paddr;

            status = BS->AllocatePages(AllocateAddress, EfiLoaderData,
                                        num_pages, &seg_addr);
            if (EFI_ERROR(status)) {
                con_print(L"[-] Failed to allocate pages for segment.\r\n");
                BS->FreePool(file_buf);
                goto close_files;
            }

            mem_copy((void *)(UINTN)seg_addr,
                     file_buf + phdr[i].p_offset,
                     (UINTN)phdr[i].p_filesz);

            if (phdr[i].p_memsz > phdr[i].p_filesz) {
                mem_set((void *)(UINTN)(seg_addr + phdr[i].p_filesz),
                        0,
                        (UINTN)(phdr[i].p_memsz - phdr[i].p_filesz));
            }
        }

        *entry_point = ehdr->e_entry;

    } else if (elf_class == ELFCLASS64) {
        Elf64_Ehdr *ehdr = (Elf64_Ehdr *)file_buf;

        if (ehdr->e_type != ET_EXEC || ehdr->e_machine != EM_X86_64) {
            con_print(L"[-] Not an x86_64 ELF executable.\r\n");
            BS->FreePool(file_buf);
            status = EFI_LOAD_ERROR;
            goto close_files;
        }

        con_print(L"[+] Valid ELF64 x86_64 executable.\r\n");

        Elf64_Phdr *phdr = (Elf64_Phdr *)(file_buf + ehdr->e_phoff);

        for (UINT16 i = 0; i < ehdr->e_phnum; i++) {
            if (phdr[i].p_type != PT_LOAD)
                continue;

            UINTN num_pages = (UINTN)((phdr[i].p_memsz + 4095) / 4096);
            EFI_PHYSICAL_ADDRESS seg_addr = phdr[i].p_paddr;

            status = BS->AllocatePages(AllocateAddress, EfiLoaderData,
                                        num_pages, &seg_addr);
            if (EFI_ERROR(status)) {
                con_print(L"[-] Failed to allocate pages for segment.\r\n");
                BS->FreePool(file_buf);
                goto close_files;
            }

            mem_copy((void *)(UINTN)seg_addr,
                     file_buf + phdr[i].p_offset,
                     (UINTN)phdr[i].p_filesz);

            if (phdr[i].p_memsz > phdr[i].p_filesz) {
                mem_set((void *)(UINTN)(seg_addr + phdr[i].p_filesz),
                        0,
                        (UINTN)(phdr[i].p_memsz - phdr[i].p_filesz));
            }
        }

        *entry_point = (UINT32)ehdr->e_entry;

    } else {
        con_print(L"[-] Unsupported ELF class.\r\n");
        BS->FreePool(file_buf);
        status = EFI_LOAD_ERROR;
        goto close_files;
    }

    con_print(L"[+] Kernel loaded successfully.\r\n");

    BS->FreePool(file_buf);

close_files:
    kernel_file->Close(kernel_file);
    root->Close(root);
    return status;
}
