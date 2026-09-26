/* scan_diag_v3.c - Scan SMEM for Items 402 & 474, and extract all crash strings */
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
    buf[0] = '0'; buf[1] = 'x';
    for (int i = 15; i >= 0; i--) {
        int nib = (val >> (i * 4)) & 0xf;
        buf[17 - i] = (nib < 10) ? ('0' + nib) : ('a' + nib - 10);
    }
    buf[18] = 0;
    print_str(buf);
}

static void dump_strings(const unsigned char *p, unsigned long len) {
    for (unsigned long i = 0; i < len; ) {
        while (i < len && (p[i] < 32 || p[i] > 126) && p[i] != '\n' && p[i] != '\r')
            i++;
        unsigned long start = i;
        while (i < len && ((p[i] >= 32 && p[i] <= 126) || p[i] == '\n' || p[i] == '\r'))
            i++;
        if (i - start >= 4) {
            print_str("  [STR+");
            print_hex(start);
            print_str("] ");
            my_syscall3(__NR_write, 1, (long)&p[start], i - start);
            print_str("\n");
        }
    }
}

static void scan_smem(const unsigned char *smem, unsigned long smem_size) {
    print_str("\n=== Scanning SMEM for Crash Items (402, 421, 474) ===\n");
    for (unsigned long i = 0; i + 12 < smem_size; i += 4) {
        unsigned short canary = *(const unsigned short *)(smem + i);
        unsigned short id     = *(const unsigned short *)(smem + i + 2);
        unsigned int size     = *(const unsigned int *)(smem + i + 4);
        if (canary == 0xa5a5 && (id == 402 || id == 421 || id == 474) && size > 0 && size <= 16384) {
            unsigned short pad_hdr = *(const unsigned short *)(smem + i + 10);
            const unsigned char *data = smem + i + 12 + pad_hdr;
            print_str("FOUND SMEM Item ");
            print_hex(id);
            print_str(" at phys=");
            print_hex(0x86000000 + i);
            print_str(" size=");
            print_hex(size);
            print_str("\n");

            print_str("--- Strings ---\n");
            dump_strings(data, size);
        }
    }
}

void _start(void) {
    print_str("=== ZethraOS SMEM Crash Inspector v3 ===\n");

    int fd = (int)my_syscall4(__NR_openat, AT_FDCWD, (long)"/dev/mem", O_RDONLY | O_SYNC, 0);
    if (fd < 0) {
        print_str("ERROR: Cannot open /dev/mem\n");
        my_syscall1(__NR_exit_group, 1);
    }

    void *map = (void *)my_syscall6(__NR_mmap, 0, 0x00200000, PROT_READ, MAP_SHARED, fd, 0x86000000);
    if (map == MAP_FAILED) {
        print_str("ERROR: mmap SMEM failed\n");
        my_syscall1(__NR_exit_group, 1);
    }

    scan_smem((const unsigned char *)map, 0x00200000);

    my_syscall2(__NR_munmap, (long)map, 0x00200000);
    my_syscall1(__NR_close, fd);
    my_syscall1(__NR_exit_group, 0);
}
