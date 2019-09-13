use bindgen;
use cc;
use shlex;

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::{env, fs};

const INCLUDED_TYPES: &[&str] = &["file_system_type", "mode_t", "umode_t", "ctl_table"];
const INCLUDED_FUNCTIONS: &[&str] = &[
    "cdev_add",
    "cdev_init",
    "cdev_del",
    "register_filesystem",
    "unregister_filesystem",
    "krealloc",
    "kfree",
    "mount_nodev",
    "kill_litter_super",
    "register_sysctl",
    "unregister_sysctl_table",
    "access_ok",
    "_copy_to_user",
    "_copy_from_user",
    "alloc_chrdev_region",
    "unregister_chrdev_region",
];
const INCLUDED_VARS: &[&str] = &[
    "EINVAL",
    "ENOMEM",
    "ESPIPE",
    "EFAULT",
    "__this_module",
    "FS_REQUIRES_DEV",
    "FS_BINARY_MOUNTDATA",
    "FS_HAS_SUBTYPE",
    "FS_USERNS_MOUNT",
    "FS_RENAME_DOES_D_MOVE",
    "BINDINGS_GFP_KERNEL",
    "KERN_INFO",
    "VERIFY_WRITE",
    "LINUX_VERSION_CODE",
    "SEEK_SET",
    "SEEK_CUR",
    "SEEK_END",
];
const OPAQUE_TYPES: &[&str] = &[
    // These need to be opaque because they're both packed and aligned, which rustc
    // doesn't support yet. See https://github.com/rust-lang/rust/issues/59154
    // and https://github.com/rust-lang/rust-bindgen/issues/1538
    "desc_struct",
    "xregs_state",
];

fn kernel_version_code(major: u8, minor: u8, patch: u8) -> u64 {
    ((major as u64) << 16) | ((minor as u64) << 8) | (patch as u64)
}

fn handle_kernel_version_cfg(bindings_path: &PathBuf) {
    let f = BufReader::new(fs::File::open(bindings_path).unwrap());
    let mut version = None;
    for line in f.lines() {
        let line = line.unwrap();
        if let Some(type_and_value) = line.split("pub const LINUX_VERSION_CODE").nth(1) {
            if let Some(value) = type_and_value.split("=").nth(1) {
                let raw_version = value.split(";").next().unwrap();
                version = Some(raw_version.trim().parse::<u64>().unwrap());
                break;
            }
        }
    }
    let version = version.expect("Couldn't find kernel version");
    if version >= kernel_version_code(4, 15, 0) {
        println!("cargo:rustc-cfg=kernel_4_15_0_or_greater")
    }
    if version >= kernel_version_code(4, 19, 0) {
        println!("cargo:rustc-cfg=kernel_4_19_0_or_greater")
    }
    if version >= kernel_version_code(4, 20, 0) {
        println!("cargo:rustc-cfg=kernel_4_20_0_or_greater")
    }
    if version >= kernel_version_code(5, 1, 0) {
        println!("cargo:rustc-cfg=kernel_5_1_0_or_greater")
    }
}

fn prepare_cflags(cflags: &str, kernel_dir: &str) -> Vec<String> {
    let cflag_parts = shlex::split(&cflags).unwrap();
    let mut cflag_iter = cflag_parts.iter();
    let mut kernel_args = vec![];
    while let Some(arg) = cflag_iter.next() {
        if arg.starts_with("-I") {
            kernel_args.push(format!("-I{}/{}", kernel_dir, &arg[2..]));
        } else if arg == "-include" {
            kernel_args.push(arg.to_string());
            kernel_args.push(format!("{}/{}", kernel_dir, cflag_iter.next().unwrap()));
        } else {
            kernel_args.push(arg.to_string());
        }
    }
    return kernel_args;
}

fn main() {
    println!("cargo:rerun-if-env-changed=KDIR");
    println!("cargo:rerun-if-env-changed=CLANG");
    println!("cargo:rerun-if-env-changed=KERNEL_CLFAGS");
    println!("cargo:rerun-if-changed=kernel-cflags-finder/Makefile");

    let kernel_cflags = env::var("KERNEL_CLFAGS").expect("Must be invoked from kernel makefile");
    let kernel_dir = env::var("KDIR").expect("Must be invoked from kernel makefile");
    let kernel_args = prepare_cflags(&kernel_cflags, &kernel_dir);

    let mut builder = bindgen::Builder::default()
        .use_core()
        .ctypes_prefix("c_types")
        .derive_default(true)
        .rustfmt_bindings(true);

    // TODO: parameterize
    builder = builder.clang_arg("--target=x86_64-linux-kernel");
    for arg in kernel_args.iter() {
        builder = builder.clang_arg(arg.clone());
    }

    println!("cargo:rerun-if-changed=src/bindings_helper.h");
    builder = builder.header("src/bindings_helper.h");

    for t in INCLUDED_TYPES {
        builder = builder.whitelist_type(t);
    }
    for f in INCLUDED_FUNCTIONS {
        builder = builder.whitelist_function(f);
    }
    for v in INCLUDED_VARS {
        builder = builder.whitelist_var(v);
    }
    for t in OPAQUE_TYPES {
        builder = builder.opaque_type(t);
    }
    let bindings = builder.generate().expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    handle_kernel_version_cfg(&out_path.join("bindings.rs"));

    let mut builder = cc::Build::new();
    builder.compiler(env::var("CLANG").unwrap_or("clang".to_string()));
    builder.target("x86_64-linux-kernel");
    builder.warnings(false);
    builder.file("src/helpers.c");
    for arg in kernel_args.iter() {
        builder.flag(&arg);
    }
    builder.compile("helpers");
}
