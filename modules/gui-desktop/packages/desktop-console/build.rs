//! 将 desktop-console 的 Windows 应用图标嵌入 PE。
//!
//! eframe 运行时仍使用 256px PNG 设置窗口图标；这里补充 PE 的
//! RT_GROUP_ICON/RT_ICON 资源，使 Explorer、任务栏和直接启动的 exe
//! 不会回退为 Windows 默认窗口图标。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const APPLICATION_ICON: &str =
    "docs/design-assets/coolzhu-icons-2026-08-27/final/app-icon-cz-moon-gate-lantern-v1.ico";
const RESOURCE_ID: &str = "COOLZHU_DESKTOP_CONSOLE_APPLICATION_ICON";
const BIN_NAME: &str = "coolzhu-desktop-console";

fn main() {
    println!("cargo:rerun-if-changed={APPLICATION_ICON}");
    println!("cargo:rerun-if-env-changed=RC");
    println!("cargo:rerun-if-env-changed=WINDOWS_RC");

    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("desktop-console is expected under modules/gui-desktop/packages/");
    let icon_path = workspace_root.join(APPLICATION_ICON);
    if !icon_path.is_file() {
        panic!(
            "desktop-console application icon resource is missing: {}",
            icon_path.display()
        );
    }

    let rc = find_resource_compiler().unwrap_or_else(|error| {
        panic!("could not find Windows resource compiler (rc.exe): {error}; set RC or WINDOWS_RC")
    });
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let resource_script = out_dir.join("coolzhu-desktop-console.rc");
    let resource_object = out_dir.join("coolzhu-desktop-console.res");
    let icon_for_rc = icon_path.to_string_lossy().replace('\\', "/");
    let script = format!(
        "#define {RESOURCE_ID} 1\n{RESOURCE_ID} ICON \"{icon_for_rc}\"\n"
    );
    fs::write(&resource_script, script).unwrap_or_else(|error| {
        panic!(
            "could not write desktop-console resource script {}: {error}",
            resource_script.display()
        )
    });

    let output = Command::new(&rc)
        .args([
            "/nologo",
            "/fo",
            resource_object.to_string_lossy().as_ref(),
            resource_script.to_string_lossy().as_ref(),
        ])
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", rc.display()));
    if !output.status.success() {
        panic!(
            "rc.exe failed with {}:\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // rustc-link-arg-bin appends the .res file to this bin's MSVC linker
    // command; link.exe then embeds RT_GROUP_ICON and RT_ICON in the PE.
    println!(
        "cargo:rustc-link-arg-bin={BIN_NAME}={}",
        resource_object.display()
    );
}

fn find_resource_compiler() -> Result<PathBuf, String> {
    for variable in ["RC", "WINDOWS_RC"] {
        if let Some(value) = env::var_os(variable) {
            let candidate = PathBuf::from(value);
            if candidate.is_file() {
                return Ok(candidate);
            }
            return Err(format!(
                "{variable} points to a missing file: {}",
                candidate.display()
            ));
        }
    }

    if let Ok(output) = Command::new("where").arg("rc.exe").output() {
        if output.status.success() {
            if let Some(path) = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .map(PathBuf::from)
                .find(|path| path.is_file())
            {
                return Ok(path);
            }
        }
    }

    let mut roots = Vec::new();
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files_x86).join("Windows Kits/10/bin"));
    }
    if let Some(program_files) = env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files).join("Windows Kits/10/bin"));
    }

    let mut candidates = Vec::new();
    for root in roots {
        if let Ok(sdk_versions) = fs::read_dir(root) {
            for version in sdk_versions.flatten() {
                let x64 = version.path().join("x64/rc.exe");
                let x86 = version.path().join("x86/rc.exe");
                candidates.extend([x64, x86]);
            }
        }
    }
    candidates.sort_by(|left, right| right.cmp(left));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "Windows SDK bin directory was not found".to_string())
}
