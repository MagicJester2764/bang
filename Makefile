OVMF_PATH=/usr/share/OVMF
KERNEL=kernel.bin
QUARK_DIR=../quark

RUST_TARGET=x86_64-unknown-uefi
RUST_PROFILE=release
EFI_BIN=target/$(RUST_TARGET)/$(RUST_PROFILE)/bang.efi

ROOTFS_DIR=rootfs
ROOTFS_IMG=rootfs.img
ROOTFS_SIZE_KB=33792

build:
	cargo build --release
	cp $(EFI_BIN) BOOTX64.EFI

image: build $(ROOTFS_IMG)
	$(eval BOOT_FAT_KB := $(shell expr $(ROOTFS_SIZE_KB) + 3072))
	dd if=/dev/zero of=fat.img bs=1k count=$(BOOT_FAT_KB)
	mformat -i fat.img ::
	mmd -i fat.img ::/EFI
	mmd -i fat.img ::/EFI/BOOT
	mcopy -i fat.img BOOTX64.EFI ::/EFI/BOOT
	mcopy -i fat.img $(KERNEL) ::/kernel.bin
	mmd -i fat.img ::/drivers
	@if [ -d drivers ] && [ "$$(ls -A drivers 2>/dev/null)" ]; then \
		for f in drivers/*; do mcopy -i fat.img "$$f" ::/drivers/; done; \
	fi
	mcopy -i fat.img $(ROOTFS_IMG) ::/drivers/rootfs.img

$(ROOTFS_IMG): FORCE
	dd if=/dev/zero of=$(ROOTFS_IMG) bs=1k count=$(ROOTFS_SIZE_KB)
	mformat -i $(ROOTFS_IMG) -F ::
	@cd $(ROOTFS_DIR) && \
	find . -mindepth 1 -type d | sort | while read d; do \
		mmd -i ../$(ROOTFS_IMG) "::$$d" 2>/dev/null || true; \
	done; \
	find . -type f ! -name '.gitkeep' | while read f; do \
		mcopy -i ../$(ROOTFS_IMG) "$$f" "::$$f"; \
	done

hd: image
	$(eval HD_SECTORS := $(shell expr '(' $(ROOTFS_SIZE_KB) + 3072 + $(ROOTFS_SIZE_KB) + 2048 ')' '*' 2))
	mkgpt -o hdimage.bin --image-size $(HD_SECTORS) \
		--part fat.img --type system \
		--part $(ROOTFS_IMG) --type linux

cd: image
	mkdir -p iso
	cp fat.img iso
	xorriso -as mkisofs -R -f -e fat.img -no-emul-boot -o cdimage.iso iso

run: hd
	sudo qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -hda hdimage.bin

run-iso: cd
	sudo qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -cdrom cdimage.iso

clean:
	rm -f BOOTX64.EFI fat.img $(ROOTFS_IMG) hdimage.bin cdimage.iso
	cargo clean

# Build quark kernel, drivers, and user-space programs, then copy artifacts here
sync-quark:
	$(MAKE) -C $(QUARK_DIR) all
	cp $(QUARK_DIR)/kernel.bin $(KERNEL)
	mkdir -p drivers
	cp $(QUARK_DIR)/drivers/vga/vga.drv drivers/
	cp $(QUARK_DIR)/drivers/fat32/fat32.drv drivers/
	cp $(QUARK_DIR)/user/init/target/x86_64-unknown-none/release/init drivers/init.elf
	cp $(QUARK_DIR)/user/hello/target/x86_64-unknown-none/release/hello drivers/hello.elf
	cp $(QUARK_DIR)/user/nameserver/target/x86_64-unknown-none/release/nameserver drivers/nameserver.elf
	cp $(QUARK_DIR)/user/keyboard/target/x86_64-unknown-none/release/keyboard drivers/keyboard.elf

.PHONY: build image hd cd run run-iso clean sync-quark FORCE

FORCE:
