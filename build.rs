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

    if compile_resource(&rc_path, &resource_path) {
        println!("cargo:rustc-link-arg-bins={}", resource_path.display());
    }
}

fn resource_file_name() -> &'static str {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        "set-desto.res"
    } else {
        "set-desto-resource.o"
    }
}

fn compile_resource(rc_path: &Path, resource_path: &Path) -> bool {
    let Some(mut command) = resource_compiler_command() else {
        println!(
            "cargo:warning=Windows resource compiler not found; skipping executable icon embedding"
        );
        return false;
    };

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        command
            .arg("/nologo")
            .arg("/fo")
            .arg(resource_path)
            .arg(rc_path);
    } else {
        command
            .arg(rc_path)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(resource_path);
    }

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

    true
}

fn resource_compiler_command() -> Option<Command> {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        find_in_path("rc.exe")
            .or_else(find_windows_sdk_rc)
            .map(Command::new)
    } else {
        find_in_path("windres.exe")
            .or_else(|| find_in_path("windres"))
            .map(Command::new)
    }
}

fn find_in_path(executable_name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?).find_map(|path| {
        let candidate = path.join(executable_name);
        candidate.is_file().then_some(candidate)
    })
}

fn find_windows_sdk_rc() -> Option<PathBuf> {
    windows_sdk_dirs()
        .into_iter()
        .filter_map(|sdk_dir| {
            let bin_dir = sdk_dir.join("bin");
            let sdk_version = env::var_os("WindowsSDKVersion")
                .map(|version| bin_dir.join(version).join(target_sdk_arch()).join("rc.exe"));
            sdk_version
                .filter(|candidate| candidate.is_file())
                .or_else(|| newest_sdk_rc(&bin_dir))
        })
        .next()
}

fn windows_sdk_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    for var_name in ["WindowsSdkDir", "WindowsSDKDir"] {
        if let Some(dir) = env::var_os(var_name) {
            dirs.push(PathBuf::from(dir));
        }
    }

    for var_name in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(dir) = env::var_os(var_name) {
            dirs.push(PathBuf::from(dir).join("Windows Kits").join("10"));
        }
    }

    dirs
}

fn newest_sdk_rc(bin_dir: &Path) -> Option<PathBuf> {
    let mut candidates = fs::read_dir(bin_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(target_sdk_arch()).join("rc.exe"))
        .filter(|candidate| candidate.is_file())
        .collect::<Vec<_>>();

    candidates.sort();
    candidates.pop()
}

fn target_sdk_arch() -> &'static str {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    }
}
