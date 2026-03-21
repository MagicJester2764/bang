#OVMF_PATH=/usr/share/OVMF
OVMF_PATH=./firmware-redist/ovmf
KERNEL=kernel.bin
QUARK_DIR=../quark

RUST_TARGET=x86_64-unknown-uefi
RUST_PROFILE=release
EFI_BIN=target/$(RUST_TARGET)/$(RUST_PROFILE)/bang.efi

ROOTFS_DIR=rootfs
ROOTFS_IMG=rootfs.img
ROOTFS_SIZE_KB=33792

BOOT_DIR=rootfs/boot
BOOT_IMG=boot.img
BOOT_IMG_SIZE_KB=1024

build:
	cargo build --release
	cp $(EFI_BIN) BOOTX64.EFI

image: build $(BOOT_IMG) $(ROOTFS_IMG)
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
	mcopy -i fat.img $(BOOT_IMG) ::/drivers/boot.img

$(BOOT_IMG): FORCE
	dd if=/dev/zero of=$(BOOT_IMG) bs=1k count=$(BOOT_IMG_SIZE_KB)
	mformat -i $(BOOT_IMG) -F ::
	@if [ -d $(BOOT_DIR) ]; then \
		cd $(BOOT_DIR) && \
		find . -type f ! -name '.gitkeep' | while read f; do \
			mcopy -i ../../$(BOOT_IMG) "$$f" "::$$f"; \
		done; \
	fi

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

ROOTFS_EXT2_IMG=rootfs-ext2.img

$(ROOTFS_EXT2_IMG): FORCE
	dd if=/dev/zero of=$(ROOTFS_EXT2_IMG) bs=1k count=$(ROOTFS_SIZE_KB)
	mkfs.ext2 -b 1024 -F -q $(ROOTFS_EXT2_IMG)
	debugfs -w -R "mkdir usr" $(ROOTFS_EXT2_IMG)
	debugfs -w -R "mkdir usr/bin" $(ROOTFS_EXT2_IMG)
	debugfs -w -R "mkdir etc" $(ROOTFS_EXT2_IMG)
	debugfs -w -R "mkdir home" $(ROOTFS_EXT2_IMG)
	debugfs -w -R "mkdir home/root" $(ROOTFS_EXT2_IMG)
	@cd $(ROOTFS_DIR) && \
	find . -type f ! -name '.gitkeep' | while read f; do \
		target=$$(echo "$$f" | sed 's|^\./||'); \
		debugfs -w -R "write $$f $$target" ../$(ROOTFS_EXT2_IMG); \
	done

hd: image $(ROOTFS_EXT2_IMG)
	$(eval HD_SECTORS := $(shell expr '(' $(ROOTFS_SIZE_KB) + 3072 + $(ROOTFS_SIZE_KB) + 2048 ')' '*' 2))
	mkgpt -o hdimage.bin --image-size $(HD_SECTORS) \
		--part fat.img --type system \
		--part $(ROOTFS_EXT2_IMG) --type linux

hd-fat32: image
	$(eval HD_SECTORS := $(shell expr '(' $(ROOTFS_SIZE_KB) + 3072 + $(ROOTFS_SIZE_KB) + 2048 ')' '*' 2))
	mkgpt -o hdimage.bin --image-size $(HD_SECTORS) \
		--part fat.img --type system \
		--part $(ROOTFS_IMG) --type linux

cd: image
	mkdir -p iso
	cp fat.img iso
	xorriso -as mkisofs -R -f -e fat.img -no-emul-boot -o cdimage.iso iso

run: hd
	qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -device rtl8139,netdev=n -netdev user,id=n -hda hdimage.bin

run-fat32: hd-fat32
	qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -device rtl8139,netdev=n -netdev user,id=n -hda hdimage.bin

run-iso: cd
	qemu-system-x86_64 -L $(OVMF_PATH)/ -pflash $(OVMF_PATH)/OVMF_CODE.fd -device rtl8139,netdev=n -netdev user,id=n -cdrom cdimage.iso

clean:
	rm -f BOOTX64.EFI fat.img $(BOOT_IMG) $(ROOTFS_IMG) $(ROOTFS_EXT2_IMG) hdimage.bin cdimage.iso
	cargo clean

# Build quark kernel, drivers, and user-space programs, then copy artifacts here
sync-quark:
	$(MAKE) -C $(QUARK_DIR) all
	cp $(QUARK_DIR)/kernel.bin $(KERNEL)
	mkdir -p drivers
	cp $(QUARK_DIR)/drivers/vga/vga.drv drivers/
	cp $(QUARK_DIR)/drivers/fat32/fat32.drv drivers/
	cp $(QUARK_DIR)/user/init/target/x86_64-unknown-none/release/init drivers/init.elf
	mkdir -p $(BOOT_DIR)
	cp $(QUARK_DIR)/user/nameserver/target/x86_64-unknown-none/release/nameserver $(BOOT_DIR)/NAMESRVR.ELF
	cp $(QUARK_DIR)/user/keyboard/target/x86_64-unknown-none/release/keyboard $(BOOT_DIR)/KEYBOARD.ELF
	cp $(QUARK_DIR)/user/console/target/x86_64-unknown-none/release/console $(BOOT_DIR)/CONSOLE.ELF
	cp $(QUARK_DIR)/user/input/target/x86_64-unknown-none/release/input $(BOOT_DIR)/INPUT.ELF
	cp $(QUARK_DIR)/user/disk/target/x86_64-unknown-none/release/disk $(BOOT_DIR)/DISK.ELF
	cp $(QUARK_DIR)/user/vfs/target/x86_64-unknown-none/release/vfs $(BOOT_DIR)/VFS.ELF
	cp $(QUARK_DIR)/user/net/target/x86_64-unknown-none/release/net $(BOOT_DIR)/NET.ELF
	mkdir -p $(ROOTFS_DIR)/usr/bin
	cp $(QUARK_DIR)/user/hello/target/x86_64-unknown-none/release/hello $(ROOTFS_DIR)/usr/bin/HELLO.ELF
	cp $(QUARK_DIR)/user/disktest/target/x86_64-unknown-none/release/disktest $(ROOTFS_DIR)/usr/bin/DISKTEST.ELF
	cp $(QUARK_DIR)/user/shell/target/x86_64-unknown-none/release/shell $(ROOTFS_DIR)/usr/bin/SHELL.ELF
	cp $(QUARK_DIR)/user/echo/target/x86_64-unknown-none/release/echo $(ROOTFS_DIR)/usr/bin/ECHO.ELF
	cp $(QUARK_DIR)/user/ls/target/x86_64-unknown-none/release/ls $(ROOTFS_DIR)/usr/bin/LS.ELF
	cp $(QUARK_DIR)/user/cat/target/x86_64-unknown-none/release/cat $(ROOTFS_DIR)/usr/bin/CAT.ELF
	cp $(QUARK_DIR)/user/login/target/x86_64-unknown-none/release/login $(ROOTFS_DIR)/usr/bin/LOGIN.ELF
	cp $(QUARK_DIR)/user/ps/target/x86_64-unknown-none/release/ps $(ROOTFS_DIR)/usr/bin/PS.ELF
	cp $(QUARK_DIR)/user/ipcping/target/x86_64-unknown-none/release/ipcping $(ROOTFS_DIR)/usr/bin/IPCPING.ELF
	cp $(QUARK_DIR)/user/ping/target/x86_64-unknown-none/release/ping $(ROOTFS_DIR)/usr/bin/PING.ELF
	cp $(QUARK_DIR)/user/httpget/target/x86_64-unknown-none/release/httpget $(ROOTFS_DIR)/usr/bin/HTTPGET.ELF
	cp $(QUARK_DIR)/user/shutdown/target/x86_64-unknown-none/release/shutdown $(ROOTFS_DIR)/usr/bin/SHUTDOWN.ELF
	mkdir -p $(ROOTFS_DIR)/etc
	cp $(QUARK_DIR)/rootfs/etc/passwd $(ROOTFS_DIR)/etc/PASSWD
	mkdir -p $(ROOTFS_DIR)/home/root

.PHONY: build image hd hd-fat32 cd run run-iso run-fat32 clean sync-quark boot FORCE

FORCE:
