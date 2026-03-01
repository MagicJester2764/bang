#!/bin/sh

export EFI_PATH=/usr/include/efi

# Compile
x86_64-w64-mingw32-gcc -ffreestanding -I$EFI_PATH/ -I$EFI_PATH/x86_64 -I$EFI_PATH/protocol -c -o hello.o hello.c
x86_64-w64-mingw32-gcc -ffreestanding -I$EFI_PATH/ -I$EFI_PATH/x86_64 -I$EFI_PATH/protocol -c -o data.o data.c

# Link
x86_64-w64-mingw32-gcc -ffreestanding -nostdlib -Wl,-dll -shared -Wl,--subsystem,10 -e efi_main -o BOOTX64.EFI hello.o data.o

