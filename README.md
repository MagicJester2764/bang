# Bang

A UEFI bootloader for x86-64. It loads ELF kernels by Multiboot1 or
Multiboot2, boots Linux through the EFI handover protocol, starts any other
UEFI application, and offers a menu of whatever it was told about and whatever
it found installed.

It was written to boot [Quark](https://github.com/MagicJester2764/quark), and
it builds one file: `BOOTX64.EFI`. Putting that file on a disk image, beside a
kernel, is a distribution's job —
[ExplOSion](https://github.com/MagicJester2764/explosion) does it for Quark.

## What it does

Bang is an ordinary UEFI application. Started by the firmware, it:

1. Reads `\bang.cfg` from the volume it was loaded from. Without one it has a
   single built-in entry, a Multiboot2 kernel at `\kernel.bin`
2. Looks at every filesystem the firmware can see for other loaders — Windows
   at the path it always uses, and shim, GRUB, systemd-boot, rEFInd or a
   fallback `BOOTX64.EFI` in any directory under `\EFI` — and adds what it
   finds to the list, leaving out itself and anything the configuration
   already names
3. Shows a menu: a numbered list and a countdown. Up and down move, a number
   picks, enter boots, and when the countdown runs out the default goes
4. Boots the choice, one of three ways

**A Multiboot kernel.** The ELF is parsed and its `PT_LOAD` segments placed;
every file in the entry's `modules` directory is loaded as a boot module; the
GOP framebuffer is asked for its mode; boot services are exited; and a
Multiboot1 or Multiboot2 information structure is built from the final memory
map, with the modules and the framebuffer in it. A kernel with a Multiboot
header is entered in 32-bit protected mode, through a trampoline that takes the
CPU down from long mode; a 64-bit ELF without one is entered directly, in long
mode.

**Linux.** A bzImage is loaded by the EFI handover protocol: the setup header
is copied into a `boot_params` page, the kernel, an initrd and a command line
are placed, and the entry point the header names is called. Boot services stay
up, because the kernel exits them itself.

**Anything else.** The image is handed back to the firmware and started, by
device path so that it can find its own files. That is how Windows boots, and
GRUB, and a UEFI shell. If it returns, so does Bang.

## Configuration

`\bang.cfg` is lines of `directive argument`; blank lines and `#` comments are
ignored.

```
timeout 5
default Quark

entry Quark
    multiboot \kernel.bin
    modules   \drivers

entry UEFI Shell
    chainload \EFI\tools\Shell.efi

entry Fedora
    linux   \vmlinuz
    initrd  \initrd.img
    options root=/dev/sda2 ro
```

| Directive | |
|---|---|
| `timeout N` | Seconds before the default boots. 5 if absent |
| `default TITLE` | Which entry that is. The first, if absent |
| `autodetect off` | Offer only what is written here |
| `entry TITLE` | Starts an entry; the lines after it describe it |
| `multiboot PATH` | An ELF kernel with a Multiboot header |
| `modules DIR` | A directory whose files are all loaded as boot modules |
| `chainload PATH` | A UEFI application to start |
| `linux PATH`, `initrd PATH`, `options ...` | A bzImage, its initrd and its command line |

A line it does not understand is reported and skipped, not refused: a
bootloader that will not start because of one bad line in its configuration is
a bootloader you cannot fix without another one.

## Booting Quark

Quark's kernel carries a Multiboot2 header, so Bang enters it in protected mode
and the kernel takes itself to long mode. The boot volume ExplOSion assembles
looks like this:

```
\EFI\BOOT\BOOTX64.EFI     Bang
\bang.cfg                 the menu
\kernel.bin               the Quark kernel
\drivers\
  vga.drv  fat32.drv      modules the kernel loads itself
  init.elf                the first program
  boot.img                a small FAT32 image holding the services init starts
```

Everything in `\drivers` reaches the kernel as a Multiboot module. The kernel
finds `init` among them by name and starts it; `init` finds `boot.img` and
starts the rest.

## Source layout

| File | Purpose |
|------|---------|
| `src/main.rs` | Entry point and boot flow |
| `src/config.rs` | `\bang.cfg` |
| `src/discover.rs` | Finding installed loaders on every volume |
| `src/menu.rs` | The menu |
| `src/elf.rs` | ELF32/64 parsing, segment loading, Multiboot detection |
| `src/modules.rs` | Loading a directory of boot modules |
| `src/multiboot.rs` | Multiboot1 and Multiboot2 information structures |
| `src/gop.rs` | GOP framebuffer queries |
| `src/trampoline.rs` | Long mode down to 32-bit protected mode |
| `src/handoff.rs` | 64-bit direct handoff |
| `src/linux.rs` | The EFI handover protocol |
| `src/chain.rs` | Starting another UEFI application |
| `src/console.rs` | Boot banner |

## Building

- **Rust nightly**, the one `rust-toolchain.toml` pins, with the
  `x86_64-unknown-uefi` target and `rust-src`

```bash
make build    # BOOTX64.EFI
make clean
```

That is all this repository builds. To make a disk image and boot it, see
ExplOSion: `make run` there stages Bang, the kernel and the userland,
assembles a GPT image and starts it in QEMU.

`firmware-redist/ovmf/` holds a copy of the OVMF firmware, because a bootloader
is what needs it to exist; ExplOSion's QEMU targets use this copy by default.

## Not tested

Windows. The path it boots by is the standard one and the mechanism is the one
GRUB and a UEFI shell are started by, but there has been no Windows
installation to try it on.

## Disclaimer

This is primarily an AI-assisted experimental project, not a production bootloader. It was built as a vehicle for exploring OS development concepts with AI tooling. Use at your own risk.
