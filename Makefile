OVMF_PATH=/usr/share/OVMF
KERNEL=kernel.bin

RUST_TARGET=x86_64-unknown-uefi
RUST_PROFILE=release
EFI_BIN=rust/target/$(RUST_TARGET)/$(RUST_PROFILE)/bang.efi

build:
	cd rust && cargo build --release
	cp $(EFI_BIN) BOOTX64.EFI

image: build
	dd if=/dev/zero of=fat.img bs=1k count=1440
	mformat -i fat.img -f 1440 ::
	mmd -i fat.img ::/EFI
	mmd -i fat.img ::/EFI/BOOT
	mcopy -i fat.img BOOTX64.EFI ::/EFI/BOOT
	mcopy -i fat.img $(KERNEL) ::/kernel.bin

hd: image
	mkgpt -o hdimage.bin --image-size 4096 --part fat.img --type system

cd: image
	mkdir -p iso
	cp fat.img iso
	xorriso -as mkisofs -R -f -e fat.img -no-emul-boot -o cdimage.iso iso

run: hd
	sudo qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -hda hdimage.bin

run-iso: cd
	sudo qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -cdrom cdimage.iso

clean:
	rm -f BOOTX64.EFI fat.img hdimage.bin cdimage.iso
	cd rust && cargo clean

.PHONY: build image hd cd run run-iso clean
