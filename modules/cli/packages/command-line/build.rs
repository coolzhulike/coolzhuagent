use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const RELEASE_VERSION_ENV: &str = "COOLZHU_RELEASE_VERSION";
const BUILD_DATE_ENV: &str = "COOLZHU_BUILD_DATE";
const GIT_SHA_ENV: &str = "COOLZHU_GIT_SHA";
const BUILD_TARGET_ENV: &str = "COOLZHU_BUILD_TARGET";

fn main() {
    for name in [
        RELEASE_VERSION_ENV,
        BUILD_DATE_ENV,
        GIT_SHA_ENV,
        BUILD_TARGET_ENV,
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    emit_git_rerun_paths(&manifest_dir);
    let release_version = non_empty_env(RELEASE_VERSION_ENV)
        .or_else(|| non_empty_env("CARGO_PKG_VERSION"))
        .unwrap_or_else(|| "unknown".to_string());
    let build_date = non_empty_env(BUILD_DATE_ENV).unwrap_or_else(current_utc_date);
    let git_sha = non_empty_env(GIT_SHA_ENV)
        .or_else(|| git_stdout(&manifest_dir, &["rev-parse", "--short=12", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());
    let build_target = non_empty_env(BUILD_TARGET_ENV)
        .or_else(|| non_empty_env("TARGET"))
        .unwrap_or_else(|| "unknown".to_string());

    emit_rustc_env("COOLZHU_RELEASE_VERSION", &release_version);
    emit_rustc_env("COOLZHU_BUILD_DATE", &build_date);
    emit_rustc_env("COOLZHU_GIT_SHA", &git_sha);
    emit_rustc_env("COOLZHU_BUILD_TARGET", &build_target);
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn git_stdout(working_dir: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(working_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn emit_git_rerun_paths(working_dir: &str) {
    let Some(git_dir) =
        git_stdout(working_dir, &["rev-parse", "--absolute-git-dir"]).map(PathBuf::from)
    else {
        return;
    };
    let head = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    println!(
        "cargo:rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );

    let Ok(head_text) = fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = head_text.trim().strip_prefix("ref: ") else {
        return;
    };
    let reference_path = git_common_dir(working_dir)
        .unwrap_or(git_dir)
        .join(Path::new(reference));
    println!("cargo:rerun-if-changed={}", reference_path.display());
}

fn git_common_dir(working_dir: &str) -> Option<PathBuf> {
    git_stdout(
        working_dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map(PathBuf::from)
}

fn emit_rustc_env(name: &str, value: &str) {
    let sanitized = value.replace(['\r', '\n'], "");
    println!("cargo:rustc-env={name}={sanitized}");
}

fn current_utc_date() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let (year, month, day) = civil_date_from_unix_days((seconds / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

// Howard Hinnant 的 civil_from_days 算法；输入为自 1970-01-01 起的 UTC 天数。
fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_piece = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}
