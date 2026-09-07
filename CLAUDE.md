# Working on Bang

Bang is the UEFI bootloader for the Quark microkernel, and it also owns the
disk image and the QEMU targets — so this is where the whole system is built
and run from, even though most of the code lives next door.

**Read `../quark/CLAUDE.md` first.** It covers the toolchain pin, the
`library/backtrace` submodule in `../rust` whose absence fails misleadingly,
and the kernel invariants. This file only adds what is specific to Bang.

The three repos must be flat siblings; `sync-quark` reaches `../quark` and the
rust fork patches `quark-rt` through a relative path.

## Targets

```bash
make sync-quark   # build ../quark and copy kernel + drivers + user programs here
make hd           # assemble hdimage.bin (GPT: EFI system partition + ext2 rootfs)
make run          # boot it in QEMU
```

`make sync-quark` is the slow one — it compiles `std` from source for the
hosted `x86_64-unknown-quark` target. If it fails, check that it actually
reached the `cp ../quark/kernel.bin` step: an earlier failure leaves a stale
`kernel.bin` in place and the image boots the old kernel, which looks like the
change you just made did nothing.

The QEMU targets pass `-cpu max` deliberately. Default CPU models expose
neither SMEP nor SMAP, so the kernel's supervisor-mode protections are silently
inactive without it.

## Untracked by design

`drivers/` and `rootfs/{boot,etc,usr}` hold artifacts `sync-quark` copies from
the quark tree. They are build outputs, regenerated on every sync, and are
gitignored — do not stage them with a broad `git add`.

## Loading

Bang parses the kernel image and everything in `\drivers\` off the boot volume,
so all of it is untrusted input: bounds-check program headers and segment
copies against the file length rather than trusting the ELF. `wcslen` in
`src/main.rs` exists only for the linker — newer toolchains rewrite the uefi
crate's UCS-2 scan loops into a libcall that nothing provides on a freestanding
target.
