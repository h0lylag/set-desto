use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/set-desto.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_icon();
    }
}

fn embed_windows_icon() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set"));
    let rc_path = out_dir.join("set-desto.rc");
    let resource_path = out_dir.join(resource_file_name());
    let icon_path = Path::new("assets").join("set-desto.ico");
    let escaped_icon_path = icon_path.display().to_string().replace('\\', "\\\\");

    fs::write(&rc_path, format!("1 ICON \"{escaped_icon_path}\"\n"))
        .expect("failed to write Windows icon resource script");

    compile_resource(&rc_path, &resource_path);
    println!("cargo:rustc-link-arg-bins={}", resource_path.display());
}

fn resource_file_name() -> &'static str {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        "set-desto.res"
    } else {
        "set-desto-resource.o"
    }
}

fn compile_resource(rc_path: &Path, resource_path: &Path) {
    let mut command = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let mut command = Command::new("rc");
        command
            .arg("/nologo")
            .arg("/fo")
            .arg(resource_path)
            .arg(rc_path);
        command
    } else {
        let mut command = Command::new("windres");
        command
            .arg(rc_path)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(resource_path);
        command
    };

    let output = command
        .output()
        .expect("failed to run Windows resource compiler");

    if !output.status.success() {
        panic!(
            "Windows resource compiler failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
