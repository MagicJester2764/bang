# Working on Bang

Bang is a UEFI bootloader. It builds `BOOTX64.EFI` and nothing else.

It used to own the disk image and the QEMU targets, so the whole system was
built from here. That moved to `../explosion`, which stages this alongside the
kernel from `../quark` and the userland from `../quarkutils` and assembles an
image out of the three. Bang does not reach into a sibling repo, and nothing
here knows an image exists.

`../quark/CLAUDE.md` is about the kernel Bang most often boots and
`../quarkutils/CLAUDE.md` about what runs on it. Neither is needed to work
here; this file is what is specific to Bang.

## Toolchain

Pinned to `nightly-2026-03-01` in `rust-toolchain.toml`, the same nightly its
siblings pin, for a reason of its own: newer toolchains rewrite the uefi
crate's UCS-2 scan loops into a `wcslen` libcall that nothing provides on a
freestanding target. `wcslen` in `src/main.rs` exists only for the linker.
Bang does not depend on the std fork; keeping the pins equal only saves rustup
a second download.

## Targets

```bash
make build   # cargo build + cp to BOOTX64.EFI
make clean
```

To build and run a whole system, go to `../explosion` and `make run`.

The OVMF firmware in `firmware-redist/` stays here, because a bootloader is
what needs it to exist. ExplOSion's QEMU targets point at this copy by default.

## What it boots

Three kinds of entry, chosen from a menu (`menu.rs`) built out of `\bang.cfg`
(`config.rs`) and whatever is found installed (`discover.rs`):

- **A Multiboot kernel** (`elf.rs`, `modules.rs`, `multiboot.rs`): boot
  services are exited here and the kernel is entered through the trampoline
  or directly.
- **Linux** (`linux.rs`), by the EFI handover protocol.
- **Any UEFI application** (`chain.rs`), by device path.

For the last two, **boot services stay up**: the image that is started needs
them and exiting them is its business. Only the Multiboot path may call
`exit_boot_services`, and everything it needs from the firmware — the modules,
the framebuffer mode — has to be read before it does.

## Things to keep true

- **A module is loaded below four gigabytes.** A Multiboot module tag has
  thirty-two bits for an address. Asked for "any pages", firmware on a
  machine with more memory than that is free to answer from above the line,
  and likes to for something large — and a module can be a whole root
  filesystem. One there is no room for below the line is skipped and said
  to be.
- **The kernel is told where the ACPI tables are.** A Multiboot2 kernel is
  handed a copy of the firmware's root pointer (tag 15, or 14 for the first
  revision's), read out of the configuration table before boot services are
  exited. Under UEFI nothing else says where the tables are, and without
  them a kernel sees one processor. Bang copies the pointer and checks only
  its signature and length; following it is the kernel's.
- **Everything read off a volume is untrusted input.** Bang parses the kernel
  image, the configuration and every file in a modules directory, so
  bounds-check program headers and segment copies against the file length
  rather than trusting the ELF, and refuse a configuration that claims to be
  enormous.
- **A bad line is skipped, not fatal.** A bootloader that will not start
  because of one line in its configuration cannot be fixed without another
  bootloader. The same goes for the menu: firmware that will not position the
  cursor gets a scrolling menu rather than none.
- **Without a `\bang.cfg` it boots `\kernel.bin`**, as it did before there was
  a configuration. An image made then still boots.
- **A configured entry stays where the user put it.** What is discovered is
  added after, and skipped if the configuration already names it.
