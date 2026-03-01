CC=x86_64-w64-mingw32-gcc
CFLAGS=-ffreestanding -I$(EFI_PATH)/ -I$(EFI_PATH)/x86_64 -I$(EFI_PATH)/protocol
EFIFLAGS=-ffreestanding -nostdlib -Wl,-dll -shared -Wl,--subsystem,10 -e efi_main
LIBS=-L/usr/lib -lefi -lgnuefi

EFI_PATH=/usr/include/efi
OVMF_PATH=/usr/share/OVMF

build:
	# Compile
	$(CC) $(CFLAGS) -c -o data.o data.c
	$(CC) $(CFLAGS) -c -o efi-local.o efi-local.c
	$(CC) $(CFLAGS) -c -o con.o con.c
	$(CC) $(CFLAGS) -c -o bang.o bang.c
	# Link
	$(CC) $(EFIFLAGS) $(LIBS) -o BOOTX64.EFI bang.o data.o efi-local.o con.o

image: build
	dd if=/dev/zero of=fat.img bs=1k count=1440
	mformat -i fat.img -f 1440 ::
	mmd -i fat.img ::/EFI
	mmd -i fat.img ::/EFI/BOOT
	mcopy -i fat.img BOOTX64.EFI ::/EFI/BOOT

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
	rm -f *.o BOOTX64.EFI fat.img hdimage.bin cdimage.iso
