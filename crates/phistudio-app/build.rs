fn main() {
    println!("cargo:rerun-if-changed=../../assets/icons");
    println!("cargo:rerun-if-changed=../../desktop/native/AppIcon.m");
    let target = std::env::var("TARGET").unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("../../desktop/native/AppIcon.m")
            .flag("-fobjc-arc")
            .compile("phistudio_app_icon");
        println!("cargo:rustc-link-lib=framework=AppKit");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        let icon = std::path::Path::new("../../assets/icons/phistudio.ico")
            .canonicalize()
            .unwrap();
        let source = out.join("app-icon.rc");
        let path = icon.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(&source, format!("1 ICON \"{path}\"\n")).unwrap();
        if target.ends_with("msvc") {
            let mut command = cc::windows_registry::find_tool(&target, "rc.exe")
                .expect("Windows SDK rc.exe is required for application icons")
                .to_command();
            let resource = out.join("app-icon.res");
            assert!(
                command
                    .arg("/nologo")
                    .arg("/c65001")
                    .arg("/fo")
                    .arg(&resource)
                    .arg(&source)
                    .status()
                    .unwrap()
                    .success()
            );
            println!("cargo:rustc-link-arg-bin=phistudio={}", resource.display());
        } else {
            println!("cargo:rerun-if-env-changed=WINDRES");
            let compiler = cc::Build::new().get_compiler();
            let windres = std::env::var_os("WINDRES")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    let name = compiler
                        .path()
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .replace("gcc", "windres");
                    compiler.path().with_file_name(name)
                });
            let resource = out.join("app-icon.o");
            assert!(
                std::process::Command::new(windres)
                    .arg("-i")
                    .arg(&source)
                    .arg("-o")
                    .arg(&resource)
                    .status()
                    .unwrap()
                    .success()
            );
            println!("cargo:rustc-link-arg-bin=phistudio={}", resource.display());
        }
    }

    println!("cargo:rerun-if-changed=../../ios/native/PhiStudioBridge.m");
    println!("cargo:rerun-if-changed=../../ios/native/PhiStudioVideo.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("../../ios/native/PhiStudioBridge.m")
            .file("../../ios/native/PhiStudioVideo.m")
            .flag("-fobjc-arc")
            .flag("-fmodules")
            .compile("phistudio_ios");
        for framework in [
            "UIKit",
            "Foundation",
            "UniformTypeIdentifiers",
            "AVFAudio",
            "AVFoundation",
            "CoreMedia",
            "CoreVideo",
            "AudioToolbox",
        ] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
