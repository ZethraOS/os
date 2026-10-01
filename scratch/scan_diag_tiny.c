/* scan_diag_tiny.c - Freestanding diagnostic scanner for Hexagon crash analysis */
#define AT_FDCWD -100
#define O_RDONLY 00
#define O_SYNC   04010000
#define PROT_READ 0x1
#define MAP_SHARED 0x01
#define MAP_FAILED ((void *)-1)

#define __NR_openat 56
#define __NR_close 57
#define __NR_write 64
#define __NR_exit_group 94
#define __NR_munmap 215
#define __NR_mmap 222

static inline long my_syscall1(long n, long a1) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a1;
    __asm__ __volatile__("svc #0" : "=r"(x0) : "r"(x8), "r"(x0) : "memory");
    return x0;
}
static inline long my_syscall2(long n, long a1, long a2) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a1;
    register long x1 __asm__("x1") = a2;
    __asm__ __volatile__("svc #0" : "=r"(x0) : "r"(x8), "r"(x0), "r"(x1) : "memory");
    return x0;
}
static inline long my_syscall3(long n, long a1, long a2, long a3) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a1;
    register long x1 __asm__("x1") = a2;
    register long x2 __asm__("x2") = a3;
    __asm__ __volatile__("svc #0" : "=r"(x0) : "r"(x8), "r"(x0), "r"(x1), "r"(x2) : "memory");
    return x0;
}
static inline long my_syscall4(long n, long a1, long a2, long a3, long a4) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a1;
    register long x1 __asm__("x1") = a2;
    register long x2 __asm__("x2") = a3;
    register long x3 __asm__("x3") = a4;
    __asm__ __volatile__("svc #0" : "=r"(x0) : "r"(x8), "r"(x0), "r"(x1), "r"(x2), "r"(x3) : "memory");
    return x0;
}
static inline long my_syscall6(long n, long a1, long a2, long a3, long a4, long a5, long a6) {
    register long x8 __asm__("x8") = n;
    register long x0 __asm__("x0") = a1;
    register long x1 __asm__("x1") = a2;
    register long x2 __asm__("x2") = a3;
    register long x3 __asm__("x3") = a4;
    register long x4 __asm__("x4") = a5;
    register long x5 __asm__("x5") = a6;
    __asm__ __volatile__("svc #0" : "=r"(x0) : "r"(x8), "r"(x0), "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5) : "memory");
    return x0;
}

static unsigned long my_strlen(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    return len;
}

static void print_str(const char *s) {
    my_syscall3(__NR_write, 1, (long)s, my_strlen(s));
}

static void print_hex(unsigned long val) {
    char buf[19];
    buf[0] = '0';
    buf[1] = 'x';
    for (int i = 15; i >= 0; i--) {
        int nib = (val >> (i * 4)) & 0xf;
        buf[17 - i] = (nib < 10) ? ('0' + nib) : ('a' + nib - 10);
    }
    buf[18] = 0;
    print_str(buf);
}

static void print_dec(unsigned long val) {
    char buf[24];
    int idx = 22;
    buf[23] = 0;
    if (val == 0) {
        print_str("0");
        return;
    }
    while (val > 0 && idx >= 0) {
        buf[idx--] = '0' + (val % 10);
        val /= 10;
    }
    print_str(&buf[idx + 1]);
}

static void dump_strings(const unsigned char *p, unsigned long len) {
    for (unsigned long i = 0; i < len; ) {
        while (i < len && (p[i] < 32 || p[i] > 126) && p[i] != '\n' && p[i] != '\r')
            i++;
        unsigned long start = i;
        while (i < len && ((p[i] >= 32 && p[i] <= 126) || p[i] == '\n' || p[i] == '\r'))
            i++;
        if (i - start >= 4) {
            print_str("  [+");
            print_hex(start);
            print_str("] ");
            my_syscall3(__NR_write, 1, (long)&p[start], i - start);
            print_str("\n");
        }
    }
}

static void inspect_smem_602(const unsigned char *smem, unsigned long smem_size) {
    print_str("\n=== Searching for SMEM Item 602 (Minidump TOC) ===\n");
    for (unsigned long i = 0; i + 16 < smem_size; i += 4) {
        if (smem[i] == 0xa5 && smem[i+1] == 0xa5 &&
            smem[i+2] == 0x5a && smem[i+3] == 0x02) {
            unsigned int size = *(const unsigned int *)(smem + i + 4);
            unsigned short pad_hdr = *(const unsigned short *)(smem + i + 10);
            print_str("FOUND SMEM Item 602 at offset ");
            print_hex(i);
            print_str(" (phys=");
            print_hex(0x86000000 + i);
            print_str(") size=");
            print_dec(size);
            print_str(" pad_hdr=");
            print_dec(pad_hdr);
            print_str("\n");

            const unsigned char *data = smem + i + 16 + pad_hdr;
            int num_entries = (size - 32) / 32;
            print_str("Total entries: ");
            print_dec(num_entries);
            print_str("\n");

            for (int e = 0; e < num_entries; e++) {
                const unsigned char *ent = data + 32 + e * 32;
                char name[17];
                for (int c = 0; c < 16; c++) name[c] = (ent[c] >= 32 && ent[c] <= 126) ? ent[c] : ' ';
                name[16] = 0;
                unsigned long base = *(const unsigned long *)(ent + 16);
                unsigned long sz = *(const unsigned long *)(ent + 24);
                if (ent[0] != 0 || base != 0 || sz != 0) {
                    print_str("Entry ");
                    print_dec(e);
                    print_str(": '");
                    print_str(name);
                    print_str("' base=");
                    print_hex(base);
                    print_str(" size=");
                    print_hex(sz);
                    print_str("\n");
                }
            }

            print_str("--- All Strings in SMEM 602 ---\n");
            dump_strings(data, size);
            return;
        }
    }
    print_str("SMEM Item 602 not found\n");
}

static void inspect_smem_log(const unsigned char *smem, unsigned long smem_size) {
    print_str("\n=== Searching for SMEM Item 79 (SMEM_LOG) ===\n");
    for (unsigned long i = 0; i + 16 < smem_size; i += 4) {
        if (smem[i] == 0xa5 && smem[i+1] == 0xa5 &&
            smem[i+2] == 0x4f && smem[i+3] == 0x00) {
            unsigned int size = *(const unsigned int *)(smem + i + 4);
            unsigned short pad_hdr = *(const unsigned short *)(smem + i + 10);
            print_str("FOUND SMEM Item 79 at offset ");
            print_hex(i);
            print_str(" size=");
            print_dec(size);
            print_str("\n");

            const unsigned char *data = smem + i + 16 + pad_hdr;
            print_str("--- Strings in SMEM_LOG ---\n");
            dump_strings(data, size);
            return;
        }
    }
}

void _start(void) {
    print_str("=== ZethraOS Minidump & SMEM Diagnostic Inspector ===\n");

    int fd = (int)my_syscall4(__NR_openat, AT_FDCWD, (long)"/dev/mem", O_RDONLY | O_SYNC, 0);
    if (fd < 0) {
        print_str("ERROR: Cannot open /dev/mem\n");
        my_syscall1(__NR_exit_group, 1);
    }

    // 1. HYP_DIAG (0x85998000, 8KB)
    {
        print_str("\n--- Reading HYP_DIAG (0x85998000) ---\n");
        void *map = (void *)my_syscall6(__NR_mmap, 0, 0x2000, PROT_READ, MAP_SHARED, fd, 0x85998000);
        if (map != MAP_FAILED) {
            const unsigned char *p = (const unsigned char *)map;
            dump_strings(p + 0x200, 0x2000 - 0x200);
            my_syscall2(__NR_munmap, (long)map, 0x2000);
        }
    }

    // 2. SMEM (0x86000000, 2MB)
    {
        print_str("\n--- Mapping SMEM (0x86000000, 2MB) ---\n");
        void *map = (void *)my_syscall6(__NR_mmap, 0, 0x00200000, PROT_READ, MAP_SHARED, fd, 0x86000000);
        if (map != MAP_FAILED) {
            const unsigned char *smem = (const unsigned char *)map;
            inspect_smem_602(smem, 0x00200000);
            inspect_smem_log(smem, 0x00200000);
            my_syscall2(__NR_munmap, (long)map, 0x00200000);
        } else {
            print_str("ERROR: mmap SMEM failed\n");
        }
    }

    print_str("\n=== Diagnostic Scan Complete ===\n");
    my_syscall1(__NR_close, fd);
    my_syscall1(__NR_exit_group, 0);
}
