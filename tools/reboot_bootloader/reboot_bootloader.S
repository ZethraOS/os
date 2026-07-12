.global _start
.text

_start:
    // Call sync() first (syscall 81 on arm64)
    mov x8, #81
    svc #0

    // Call reboot(0xfee1dead, 672274793, 0xA1B2C3D4, "bootloader")
    // x0 = 0xfee1dead (Magic 1)
    mov x0, #0xdead
    movk x0, #0xfee1, lsl #16
    
    // x1 = 672274793 = 0x28121969 (Magic 2)
    mov x1, #0x1969
    movk x1, #0x2812, lsl #16
    
    // x2 = 0xA1B2C3D4 (LINUX_REBOOT_CMD_RESTART2)
    mov x2, #0xc3d4
    movk x2, #0xa1b2, lsl #16
    
    // x3 = pointer to "bootloader"
    adr x3, bootloader_str
    
    // x8 = 142 (__NR_reboot)
    mov x8, #142
    svc #0

    // If reboot fails, exit(1) (exit is syscall 93)
    mov x0, #1
    mov x8, #93
    svc #0

bootloader_str:
    .asciz "bootloader"
