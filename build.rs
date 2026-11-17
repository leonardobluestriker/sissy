fn main() {
    println!("cargo:rerun-if-changed=src/icon_appearance.m");
    if std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("macos") {
        return;
    }
    let target = std::env::var("TARGET").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    for source in [
        "src/icon_appearance.m",
        "src/media.m",
        "src/file_operations.m",
    ] {
        println!("cargo:rerun-if-changed={source}");
        let object = format!(
            "{out_dir}/{}.o",
            std::path::Path::new(source)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
        );
        let status = std::process::Command::new("clang")
            .args([
                "-target",
                &target,
                "-mmacosx-version-min=11.0",
                "-fobjc-arc",
                "-fmodules",
                "-c",
                source,
                "-o",
                &object,
            ])
            .status()
            .expect("clang");
        if !status.success() {
            panic!("não compilou {source}");
        }
        println!("cargo:rustc-link-arg={object}");
    }
    for framework in [
        "AppKit",
        "AVKit",
        "AVFoundation",
        "Quartz",
        "QuickLookThumbnailing",
        "UniformTypeIdentifiers",
        "ImageIO",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
