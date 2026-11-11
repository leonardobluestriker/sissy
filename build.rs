fn main() {
    println!("cargo:rerun-if-changed=src/icon_appearance.m");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("macos") {
        return;
    }
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let object = format!("{out_dir}/icon_appearance.o");
    let status = std::process::Command::new("clang")
        .args([
            "-fobjc-arc",
            "-fmodules",
            "-c",
            "src/icon_appearance.m",
            "-o",
            &object,
        ])
        .status()
        .expect("clang");
    if !status.success() {
        panic!("não compilou src/icon_appearance.m");
    }
    println!("cargo:rustc-link-arg={object}");
    println!("cargo:rustc-link-lib=framework=AppKit");
}
