#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <ctype.h>

static void dump_printable(const char *label, const void *buf, size_t len) {
    const uint8_t *p = (const uint8_t *)buf;
    printf("=== %s (len=%zu) ===\n", label, len);
    for (size_t i = 0; i < len; ) {
        while (i < len && (p[i] < 32 || p[i] > 126) && p[i] != '\n' && p[i] != '\r')
            i++;
        size_t start = i;
        while (i < len && ((p[i] >= 32 && p[i] <= 126) || p[i] == '\n' || p[i] == '\r'))
            i++;
        if (i - start >= 4) {
            printf("  [0x%06zx] %.*s\n", start, (int)(i - start), &p[start]);
        }
    }
}

static void dump_hyp_diag(int fd) {
    uint64_t base = 0x85998000;
    size_t size = 0x2000;
    void *map = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, base);
    if (map == MAP_FAILED) {
        perror("mmap HYP_DIAG failed");
        return;
    }
    printf("=== HYP_DIAG (0x%08llx, size=0x%zx) ===\n", (unsigned long long)base, size);
    char *text = (char *)map + 0x200;
    size_t text_len = size - 0x200;
    dump_printable("HYP_DIAG log text", text, text_len);
    munmap(map, size);
}

static void scan_region_for_patterns(int fd, uint64_t base, size_t total_size, const char *reg_name) {
    printf("=== Scanning %s (base=0x%08llx, size=0x%zx) ===\n", reg_name, (unsigned long long)base, total_size);
    size_t chunk_size = 4 * 1024 * 1024; // 4MB chunks
    for (size_t off = 0; off < total_size; off += chunk_size) {
        size_t cur_len = (off + chunk_size <= total_size) ? chunk_size : (total_size - off);
        void *map = mmap(NULL, cur_len, PROT_READ, MAP_SHARED, fd, base + off);
        if (map == MAP_FAILED) {
            continue;
        }

        const uint8_t *p = (const uint8_t *)map;
        // Search for EF:
        for (size_t i = 0; i + 3 < cur_len; i++) {
            if (p[i] == 'E' && p[i+1] == 'F' && p[i+2] == ':') {
                printf(">>> FOUND 'EF:' at %s + 0x%zx (phys 0x%llx):\n",
                       reg_name, off + i, (unsigned long long)(base + off + i));
                size_t dump_len = 256;
                if (i + dump_len > cur_len) dump_len = cur_len - i;
                dump_printable("EF: context", p + i, dump_len);
            }
        }

        // Search for "ERR_FATAL"
        for (size_t i = 0; i + 9 < cur_len; i++) {
            if (memcmp(p + i, "ERR_FATAL", 9) == 0) {
                // Check if followed by interesting text
                printf(">>> FOUND 'ERR_FATAL' at %s + 0x%zx (phys 0x%llx):\n",
                       reg_name, off + i, (unsigned long long)(base + off + i));
                size_t dump_len = 256;
                if (i + dump_len > cur_len) dump_len = cur_len - i;
                dump_printable("ERR_FATAL context", p + i, dump_len);
            }
        }

        munmap(map, cur_len);
    }
}

static void dump_smem_log(int fd) {
    uint64_t base = 0x86007000;
    size_t size = 0x10000; // 64KB covers item 79 (40KB)
    void *map = mmap(NULL, size, PROT_READ, MAP_SHARED, fd, base);
    if (map == MAP_FAILED) {
        perror("mmap SMEM_LOG failed");
        return;
    }
    printf("=== Inspecting SMEM Item 79 / Item 80 ===\n");
    // Item 79 is at offset 0xF18 from 0x86007000
    uint8_t *item79 = (uint8_t *)map + 0xF18;
    dump_printable("SMEM Item 79 (SMEM_LOG)", item79, 40000);
    // Item 80 is at offset 0xAC8 + 16 from 0x86007000 = 0xAD8
    uint8_t *item80 = (uint8_t *)map + 0xAD8;
    dump_printable("SMEM Item 80", item80, 1024);

    munmap(map, size);
}

int main(int argc, char **argv) {
    int fd = open("/dev/mem", O_RDONLY | O_SYNC);
    if (fd < 0) {
        perror("open /dev/mem");
        return 1;
    }

    dump_hyp_diag(fd);
    dump_smem_log(fd);

    // Scan SMEM (2MB at 0x86000000)
    scan_region_for_patterns(fd, 0x86000000, 0x200000, "SMEM");

    // Scan MPSS memory (126MB at 0x8ac00000)
    scan_region_for_patterns(fd, 0x8ac00000, 0x07e00000, "MPSS");

    close(fd);
    return 0;
}
