use std::process::Command;

fn git(root: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("Cargo 必须提供 crate 路径");
    // 保留 Cargo 默认源码扫描；另外追踪工作树 HEAD、共享引用和打包引用。
    // Git 提交号只是编译来源参考，不代替含未提交文件的源码快照摘要。
    println!("cargo:rerun-if-changed=.");
    for name in ["COOLZHU_GIT_SHA", "VERGEN_GIT_SHA", "GIT_COMMIT", "GIT_HASH"] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let mut paths = vec!["HEAD".to_owned(), "packed-refs".to_owned()];
    if let Some(reference) = git(&root, &["symbolic-ref", "-q", "HEAD"]) {
        paths.push(reference);
    }
    for path in paths {
        if let Some(resolved) = git(&root, &["rev-parse", "--path-format=absolute", "--git-path", &path]) {
            println!("cargo:rerun-if-changed={resolved}");
        }
    }
    let revision = git(&root, &["rev-parse", "--verify", "HEAD"])
        .filter(|value| matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=COOLZHU_COMPILED_GIT_SHA={revision}");
}
