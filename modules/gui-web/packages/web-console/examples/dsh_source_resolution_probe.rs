//! 真实来源工程核验：调用生产解析与下载，不安装、不启动插件、不构造模型。
#[path = "../src/dsh_source_download.rs"]
mod dsh_source_download;
#[path = "../src/dsh_source_resolve.rs"]
mod dsh_source_resolve;
use dsh_source_download::DownloadControl;
use dsh_source_resolve::{prepare, resolve, SourceCandidate};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

fn control(cancelled: bool) -> DownloadControl {
    DownloadControl::new(
        Instant::now() + Duration::from_secs(120),
        Arc::new(AtomicBool::new(cancelled)),
    )
    .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("需要：独立暂存目录、输出JSON".into());
    }
    let parent = PathBuf::from(&args[0]).canonicalize()?;
    let started = Instant::now();
    let repository = "https://github.com/omdsh-dev/dsh-tool-calculator";
    let commit = "b2007a13f06bcf75bf07b9d277ee8d434a316490";
    let candidate = SourceCandidate::new("omdsh-dev", repository, None)?;
    let sdk_hash = format!(
        "{:x}",
        Sha256::digest(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/package-lock.json"
        )))
    );
    let prepared = prepare(
        &candidate,
        Some(commit),
        &sdk_hash,
        &parent,
        &control(false),
    )
    .await?;
    assert_eq!(
        prepared.package.receipt.name,
        "@deepseek-ai/dsh-tool-calculator"
    );
    assert_eq!(prepared.package.commit, commit);
    assert_eq!(prepared.package.receipt.files.len(), 22);
    let path = prepared.path().to_owned();
    prepared.package.verify(&path)?;
    let verified = json!({"package":prepared.package, "tree_sha":prepared.tree_sha,
        "license":prepared.license, "declared_scripts":prepared.declared_scripts});
    prepared.discard()?;
    assert!(!path.exists());
    let head = resolve(&candidate, None, &control(false)).await?;
    let wrong = SourceCandidate::new("omdsh-dev", repository, Some("another-package"))?;
    let mismatch = resolve(&wrong, Some(commit), &control(false))
        .await
        .unwrap_err();
    assert_eq!(mismatch.code, "source_invalid");
    let cancelled =
        match prepare(&candidate, Some(commit), &sdk_hash, &parent, &control(true)).await {
            Err(e) => e,
            Ok(_) => return Err("预取消不得下载成功".into()),
        };
    assert_eq!(cancelled.code, "download_cancelled");
    let floating = resolve(&candidate, Some("main"), &control(false))
        .await
        .unwrap_err();
    assert_eq!(floating.code, "source_invalid");
    assert_eq!(std::fs::read_dir(&parent)?.count(), 0);
    let result = json!({"kind":"工程核验，不替代安装版UI与真实Qwen验收",
        "elapsed_ms":started.elapsed().as_millis(), "fixed_source":verified,
        "head_resolved_once":head, "npm_mismatch":mismatch, "pre_cancelled":cancelled,
        "floating_commit_rejected":floating, "temporary_files_remaining":0, "discard_confirmed":true});
    std::fs::write(&args[1], serde_json::to_vec_pretty(&result)?)?;
    println!("固定源码全树核验、HEAD解析、npm冲突、预取消及暂存清理通过");
    Ok(())
}
