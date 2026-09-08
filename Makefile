# Bang — a UEFI bootloader.
#
# This builds BOOTX64.EFI and nothing else. Assembling a disk image out of it,
# and running one, belongs to whatever distro is being built — see ../explosion,
# which stages this alongside the kernel from ../quark.
#
# The OVMF firmware lives here because a bootloader is what needs it to exist;
# ExplOSion's QEMU targets point at this copy by default.

OVMF_PATH ?= ./firmware-redist/ovmf

RUST_TARGET := x86_64-unknown-uefi
RUST_PROFILE := release
EFI_BIN := target/$(RUST_TARGET)/$(RUST_PROFILE)/bang.efi

.PHONY: build clean

build:
	cargo build --release
	cp $(EFI_BIN) BOOTX64.EFI

clean:
	rm -f BOOTX64.EFI
	cargo clean
