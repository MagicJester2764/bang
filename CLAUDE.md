# Working on Bang

Bang is a UEFI bootloader. It builds `BOOTX64.EFI` and nothing else.

It used to own the disk image and the QEMU targets, so the whole system was
built from here. That moved to `../explosion`, which stages this alongside the
kernel from `../quark` and assembles an image out of both. Bang no longer
reaches into a sibling repo, and nothing here knows an image exists.

**Read `../quark/CLAUDE.md` first.** It covers the toolchain pin and the kernel
invariants. This file only adds what is specific to Bang.

## Targets

```bash
make build   # cargo build + cp to BOOTX64.EFI
make clean
```

To build and run a whole system, go to `../explosion` and `make run`.

The OVMF firmware in `firmware-redist/` stays here, because a bootloader is
what needs it to exist. ExplOSion's QEMU targets point at this copy by default.

## Loading

Bang parses the kernel image and everything in `\drivers\` off the boot volume,
so all of it is untrusted input: bounds-check program headers and segment
copies against the file length rather than trusting the ELF. `wcslen` in
`src/main.rs` exists only for the linker — newer toolchains rewrite the uefi
crate's UCS-2 scan loops into a libcall that nothing provides on a freestanding
target.
