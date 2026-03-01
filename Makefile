OVMF_PATH=/usr/share/OVMF
KERNEL=kernel.bin

RUST_TARGET=x86_64-unknown-uefi
RUST_PROFILE=release
EFI_BIN=target/$(RUST_TARGET)/$(RUST_PROFILE)/bang.efi

build:
	cargo build --release
	cp $(EFI_BIN) BOOTX64.EFI

image: build
	dd if=/dev/zero of=fat.img bs=1k count=1440
	mformat -i fat.img -f 1440 ::
	mmd -i fat.img ::/EFI
	mmd -i fat.img ::/EFI/BOOT
	mcopy -i fat.img BOOTX64.EFI ::/EFI/BOOT
	mcopy -i fat.img $(KERNEL) ::/kernel.bin
	mmd -i fat.img ::/drivers
	@if [ -d drivers ] && [ "$$(ls -A drivers 2>/dev/null)" ]; then \
		for f in drivers/*; do mcopy -i fat.img "$$f" ::/drivers/; done; \
	fi

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
	cargo clean

.PHONY: build image hd cd run run-iso clean
