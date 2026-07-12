/* UFS BSG stub for eMMC-only builds (SDM636 Nokia 6.1 Plus uses eMMC, not UFS) */
int ufs_bsg_dev_open(void) { return -1; }
int set_boot_lun(const char *bsg_node, int boot_lun) { (void)bsg_node; (void)boot_lun; return -1; }
