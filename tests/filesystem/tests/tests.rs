use std::fs;

use kernel_module_testlib::{with_kernel_module};

#[test]
fn test_printk() {
    with_kernel_module(|| {
        let filesystems = fs::read_to_string("/proc/filesystems").unwrap();
        assert!(filesystems.contains("testfs"));
    });
}
