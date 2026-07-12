#include <unistd.h>
#include <sys/syscall.h>
#include <stdio.h>

#define LINUX_REBOOT_MAGIC1     0xfee1dead
#define LINUX_REBOOT_MAGIC2     672274793
#define LINUX_REBOOT_CMD_RESTART2 0xA1B2C3D4

int main(void) {
    sync();
    printf("Rebooting to bootloader (fastboot)...\n");
    long ret = syscall(SYS_reboot, LINUX_REBOOT_MAGIC1, LINUX_REBOOT_MAGIC2,
                       LINUX_REBOOT_CMD_RESTART2, "bootloader");
    if (ret < 0) {
        perror("reboot");
        return 1;
    }
    return 0;
}
