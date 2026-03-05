# Bang

A UEFI bootloader for x86-64 that loads ELF kernels with Multiboot1 and Multiboot2 support.

## What it does

Bang is a UEFI application that:

1. Loads an ELF kernel (`kernel.bin`) from the boot volume
2. Loads driver modules from a `\drivers\` directory as Multiboot boot modules
3. Packages a FAT32 rootfs image as a boot module for the kernel
4. Queries the GOP framebuffer for display info
5. Exits UEFI boot services and builds a Multiboot1 or Multiboot2 info structure
6. Hands off to the kernel in either 32-bit protected mode (via a 64-to-32-bit trampoline) or 64-bit long mode

## How it works

Bang runs as a standard UEFI application (`BOOTX64.EFI`). It uses UEFI's `SimpleFileSystem` protocol to read the kernel and driver files from the boot FAT partition. After parsing the ELF headers to detect the kernel's Multiboot version and bitness, it allocates physical memory for each PT_LOAD segment, copies the data, and prepares a Multiboot info structure containing the memory map, framebuffer info, and module descriptors.

For 32-bit Multiboot kernels, a trampoline written in GAS intel syntax switches from 64-bit long mode down to 32-bit protected mode before jumping to the kernel entry point. For 64-bit kernels, a direct handoff passes control with the Multiboot2 info pointer in RDI.

### Boot volume layout

```
\EFI\BOOT\BOOTX64.EFI     Bang bootloader
\kernel.bin                Quark kernel (Multiboot2 ELF)
\drivers\
  init.elf                 Init process
  vga.drv                  VGA text mode driver
  fat32.drv                FAT32 filesystem driver
  rootfs.img               FAT32 rootfs (user-space ELFs inside)
```

The rootfs image contains user-space programs as FAT32 8.3 files:

```
rootfs.img (FAT32)
  NAMESRVR.ELF             Nameserver
  CONSOLE.ELF              Console server
  HELLO.ELF                Hello world test
  KEYBOARD.ELF             PS/2 keyboard driver
  INPUT.ELF                Input server (line discipline)
```

### Source layout

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point, boot flow orchestration |
| `src/elf.rs` | ELF32/64 parsing, segment loading, Multiboot detection |
| `src/multiboot.rs` | Multiboot1/2 info structure builders |
| `src/modules.rs` | Boot module loading from `\drivers\` |
| `src/gop.rs` | GOP framebuffer queries |
| `src/trampoline.rs` | 64-bit to 32-bit trampoline (global_asm) |
| `src/handoff.rs` | 64-bit direct handoff |
| `src/console.rs` | Boot banner |

## Building

### Dependencies

- **Rust nightly** with `x86_64-unknown-uefi` target and `rust-src` component
- **mtools** (`mformat`, `mmd`, `mcopy`) for FAT image creation
- **mkgpt** for GPT disk image creation
- **xorriso** for ISO image creation (optional, for `make cd`)
- **QEMU** with **OVMF** firmware for testing

### Build commands

```bash
make build       # Compile the EFI binary
make image       # Create boot FAT image with kernel and drivers
make hd          # Create GPT hard disk image (boot + rootfs partitions)
make cd          # Create bootable ISO image
make run         # Build HD image and run in QEMU
make run-iso     # Build ISO and run in QEMU
make clean       # Remove all build artifacts
make sync-quark  # Build and copy kernel + drivers + user programs from ../quark
```

### Rootfs

Files placed in the `rootfs/` directory are packaged into a FAT32 image (~33 MiB) and included both as a second GPT partition and as a boot module (so the kernel can access it without a disk driver). The `sync-quark` target populates this directory with the Quark user-space ELFs.

## Running

```bash
# Build everything from the quark kernel first
make sync-quark

# Run in QEMU
make run
```

Requires OVMF UEFI firmware installed at `/usr/share/OVMF/` (configurable via `OVMF_PATH`).

## Disclaimer

This is primarily an AI-assisted experimental project, not a production bootloader. It was built as a vehicle for exploring OS development concepts with AI tooling. Use at your own risk.
