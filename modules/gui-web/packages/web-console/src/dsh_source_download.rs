//! 固定 GitHub 来源的 DSH 包下载。只核验源码，不安装、加载或运行第三方脚本。
use futures_util::StreamExt;
use plugins::{DshPackage, DshSourceFile, DshSourceReceipt};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const MAX_FILE: usize = 4 * 1024 * 1024;
const MAX_PACKAGE: u64 = 32 * 1024 * 1024;
const MAX_TREE: usize = 512 * 1024;
const MAX_LIFETIME: Duration = Duration::from_secs(120);

#[derive(Debug, Serialize)]
pub struct DownloadError {
    pub code: &'static str,
    pub message: String,
    pub cleanup_confirmed: bool,
}
impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}：{}", self.code, self.message)
    }
}
impl std::error::Error for DownloadError {}
fn error(code: &'static str, message: impl Into<String>) -> DownloadError {
    DownloadError {
        code,
        message: message.into(),
        cleanup_confirmed: false,
    }
}
fn invalid(message: &str) -> DownloadError {
    error("source_invalid", message)
}
fn digest(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 整次操作只使用接纳时冻结的截止和取消位，不在每个文件或重试时重置。
pub struct DownloadControl {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}
impl DownloadControl {
    pub fn new(deadline: Instant, cancelled: Arc<AtomicBool>) -> Result<Self, DownloadError> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || remaining > MAX_LIFETIME {
            return Err(error("download_timeout", "来源下载的截止无效或超过120秒"));
        }
        Ok(Self {
            deadline,
            cancelled,
        })
    }
    pub(crate) fn check(&self) -> Result<(), DownloadError> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(error("download_cancelled", "来源下载已取消，未安装或启用"));
        }
        if Instant::now() >= self.deadline {
            return Err(error(
                "download_timeout",
                "来源下载总预算已耗尽，未安装或启用",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceObject {
    pub path: String,
    pub mode: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sha: String,
    #[serde(default)]
    pub size: Option<u64>,
}
#[derive(Deserialize)]
struct GitTree {
    sha: String,
    truncated: bool,
    tree: Vec<SourceObject>,
}
#[derive(Deserialize)]
struct TreeIdentity {
    sha: String,
}
#[derive(Deserialize)]
struct GitCommit {
    sha: String,
    tree: TreeIdentity,
}

/// 私有暂存目录随对象释放；只有调用者显式交给 install_dsh 才会改变插件登记。
pub struct PreparedPackage {
    directory: tempfile::TempDir,
    pub package: DshPackage,
    pub tree_sha: String,
    pub objects: Vec<SourceObject>,
    pub license: String,
    pub declared_scripts: serde_json::Value,
}
impl PreparedPackage {
    #[cfg(test)]
    pub(crate) fn cleanup_fixture(directory: tempfile::TempDir, package: DshPackage) -> Self {
        Self { directory, package, tree_sha: "a".repeat(40), objects: Vec::new(),
            license: "isolated-test-fixture".into(), declared_scripts: serde_json::Value::Null }
    }
    pub fn path(&self) -> &Path {
        self.directory.path()
    }
    pub fn discard(self) -> Result<(), DownloadError> {
        self.directory
            .close()
            .map_err(|_| error("download_cleanup_failed", "来源暂存目录删除未确认"))
    }
}

struct PackageData {
    package: DshPackage,
    tree_sha: String,
    objects: Vec<SourceObject>,
    license: String,
    declared_scripts: serde_json::Value,
}

pub(crate) async fn get(
    client: &reqwest::Client,
    url: reqwest::Url,
    limit: usize,
    control: &DownloadControl,
) -> Result<Vec<u8>, DownloadError> {
    control.check()?;
    let request = async {
        let response = client
            .get(url)
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(|_| error("download_failed", "来源连接失败或超时"))?;
        if !response.status().is_success() {
            return Err(error(
                "download_failed",
                format!(
                    "来源返回HTTP {}，未改用其它仓库或版本",
                    response.status().as_u16()
                ),
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
            || response
                .headers()
                .get(reqwest::header::CONTENT_ENCODING)
                .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"identity"))
        {
            return Err(invalid("来源响应大小或编码不受支持"));
        }
        let mut result = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            control.check()?;
            let chunk = chunk.map_err(|_| error("download_failed", "来源读取中断"))?;
            if result.len().saturating_add(chunk.len()) > limit {
                return Err(invalid("来源响应超过大小限制"));
            }
            result.extend_from_slice(&chunk);
        }
        control.check()?;
        Ok(result)
    };
    tokio::pin!(request);
    loop {
        tokio::select! { biased;
            _ = tokio::time::sleep(Duration::from_millis(25)) => control.check()?,
            result = &mut request => { control.check()?; return result; }
        }
    }
}

pub(crate) fn repository_parts(repository: &str) -> Result<(&str, &str), DownloadError> {
    let parts = repository
        .strip_prefix("https://github.com/")
        .ok_or_else(|| invalid("仅支持公开GitHub HTTPS来源"))?
        .split('/')
        .collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || p.len() > 100
                || p.starts_with('.')
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err(invalid(
            "GitHub仓库身份无效，不能使用分支路径、查询参数或其它地址",
        ));
    }
    Ok((parts[0], parts[1]))
}

fn object_hash(kind: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("{kind} {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// 重建 Git 的每一层目录对象，拒绝截断树、缺失父目录、模式变更或成员替换。
fn verify_tree(tree: &GitTree, expected: &str) -> Result<(), DownloadError> {
    if tree.sha != expected || tree.truncated || tree.tree.len() > 2048 || tree.tree.is_empty() {
        return Err(invalid("来源树身份不符、截断或超出首批包大小限制"));
    }
    let mut seen = BTreeSet::new();
    let directories = tree
        .tree
        .iter()
        .filter(|item| item.kind == "tree")
        .map(|item| (item.path.as_str(), item.sha.as_str()))
        .chain([("", expected)])
        .collect::<BTreeMap<_, _>>();
    let mut size = 0_u64;
    let mut files = 0;
    for item in &tree.tree {
        DshPackage::validate_source_path(&item.path).map_err(|e| invalid(&e.to_string()))?;
        if !seen.insert(item.path.to_lowercase()) || !digest(&item.sha, 40) {
            return Err(invalid("来源文件重复或对象摘要无效"));
        }
        let parent = item.path.rsplit_once('/').map_or("", |(parent, _)| parent);
        if !directories.contains_key(parent) {
            return Err(invalid("来源树缺少父目录"));
        }
        match (item.kind.as_str(), item.mode.as_str(), item.size) {
            ("tree", "040000", None) => (),
            ("blob", "100644" | "100755", Some(len)) if len <= MAX_FILE as u64 => {
                files += 1;
                size = size
                    .checked_add(len)
                    .ok_or_else(|| invalid("来源总大小溢出"))?;
            }
            _ => return Err(invalid("来源包含链接、子模块、未知模式或超大文件")),
        }
    }
    if !(2..=1024).contains(&files) || size > MAX_PACKAGE {
        return Err(invalid("来源包文件数量或总大小超限"));
    }
    for (directory, expected_sha) in directories {
        let mut children = tree
            .tree
            .iter()
            .filter(|item| item.path.rsplit_once('/').map_or("", |(parent, _)| parent) == directory)
            .collect::<Vec<_>>();
        let sort_key = |item: &SourceObject| {
            let name = item.path.rsplit('/').next().unwrap();
            format!("{name}{}", if item.kind == "tree" { "/" } else { "" }).into_bytes()
        };
        children.sort_by_key(|item| sort_key(item));
        let mut content = Vec::new();
        for item in children {
            let name = item.path.rsplit('/').next().unwrap();
            content.extend_from_slice(
                format!("{} {name}\0", item.mode.trim_start_matches('0')).as_bytes(),
            );
            for position in (0..40).step_by(2) {
                content.push(u8::from_str_radix(&item.sha[position..position + 2], 16).unwrap());
            }
        }
        if object_hash("tree", &content) != expected_sha {
            return Err(invalid("来源目录成员与固定Git树摘要不符"));
        }
    }
    Ok(())
}

pub async fn download(
    repository: &str,
    commit: &str,
    expected_name: &str,
    sdk_lock_sha256: &str,
    temporary_parent: &Path,
    control: &DownloadControl,
) -> Result<PreparedPackage, DownloadError> {
    control.check()?;
    let (owner, repo) = repository_parts(repository)?;
    if !digest(commit, 40)
        || !digest(sdk_lock_sha256, 64)
        || expected_name.is_empty()
        || expected_name.len() > 214
    {
        return Err(invalid("来源必须使用固定commit、明确包名与宿主SDK锁摘要"));
    }
    DshPackage::validate_source_root(temporary_parent).map_err(|e| invalid(&e.to_string()))?;
    let directory = tempfile::Builder::new()
        .prefix("dsh-source-")
        .tempdir_in(temporary_parent)
        .map_err(|_| error("download_io_failed", "无法创建私有来源暂存目录"))?;
    match download_into(
        repository,
        commit,
        expected_name,
        sdk_lock_sha256,
        owner,
        repo,
        directory.path(),
        control,
    )
    .await
    {
        Ok(data) => Ok(PreparedPackage {
            directory,
            package: data.package,
            tree_sha: data.tree_sha,
            objects: data.objects,
            license: data.license,
            declared_scripts: data.declared_scripts,
        }),
        Err(mut cause) => {
            cause.cleanup_confirmed = directory.close().is_ok();
            if !cause.cleanup_confirmed {
                cause
                    .message
                    .push_str("；来源暂存目录清理未确认，需保留该失败事实");
            }
            Err(cause)
        }
    }
}

async fn download_into(
    repository: &str,
    commit: &str,
    expected_name: &str,
    sdk_lock_sha256: &str,
    owner: &str,
    repo: &str,
    directory: &Path,
    control: &DownloadControl,
) -> Result<PackageData, DownloadError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(control.deadline.saturating_duration_since(Instant::now()))
        .user_agent("coolzhu-web-console/DSH-fixed-source")
        .build()
        .map_err(|_| error("download_failed", "无法建立来源连接"))?;
    // 忽略远程 JSON 中的下载URL，全部地址由已核验仓库、commit和相对路径组装。
    let api = format!("https://api.github.com/repos/{owner}/{repo}/git");
    let bytes = get(
        &client,
        format!("{api}/commits/{commit}").parse().unwrap(),
        MAX_TREE,
        control,
    )
    .await?;
    let identity: GitCommit =
        serde_json::from_slice(&bytes).map_err(|_| invalid("Git提交信息格式无效"))?;
    if identity.sha != commit || !digest(&identity.tree.sha, 40) {
        return Err(invalid("Git提交与请求的固定身份不符"));
    }
    let bytes = get(
        &client,
        format!("{api}/trees/{}?recursive=1", identity.tree.sha)
            .parse()
            .unwrap(),
        MAX_TREE,
        control,
    )
    .await?;
    let tree: GitTree = serde_json::from_slice(&bytes).map_err(|_| invalid("Git目录格式无效"))?;
    verify_tree(&tree, &identity.tree.sha)?;
    let mut receipt_files = Vec::new();
    let mut metadata = None;
    let mut license_found = false;
    for item in tree.tree.iter().filter(|item| item.kind == "blob") {
        control.check()?;
        let mut url: reqwest::Url = "https://raw.githubusercontent.com/".parse().unwrap();
        {
            let mut segments = url.path_segments_mut().unwrap();
            segments.pop_if_empty().push(owner).push(repo).push(commit);
            for part in item.path.split('/') {
                segments.push(part);
            }
        }
        let bytes = get(&client, url, MAX_FILE, control).await?;
        if Some(bytes.len() as u64) != item.size || object_hash("blob", &bytes) != item.sha {
            return Err(invalid("来源文件与固定Git blob身份不符，未安装"));
        }
        if item.path == "package.json" {
            metadata = Some(
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .map_err(|_| invalid("package.json无效"))?,
            );
        }
        if !bytes.is_empty()
            && (item.path == "LICENSE"
                || item.path.starts_with("LICENSE.")
                || item.path == "COPYING")
        {
            license_found = true;
        }
        let destination = directory.join(&item.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| error("download_io_failed", "暂存目录创建失败"))?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|_| error("download_io_failed", "暂存来源写入失败"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| error("download_io_failed", "暂存来源写入或同步失败"))?;
        receipt_files.push(DshSourceFile {
            path: item.path.clone(),
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        });
    }
    control.check()?;
    let metadata =
        metadata.ok_or_else(|| invalid("来源根目录缺少package.json；首批不支持monorepo子包"))?;
    let field = |name: &str| {
        metadata[name]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| invalid("包身份、入口或许可证信息缺失"))
    };
    let name = field("name")?;
    let version = field("version")?;
    let entry = DshPackage::normalize_source_entry(field("main")?)
        .map_err(|e| invalid(&e.to_string()))?;
    let license = field("license")?;
    if name != expected_name || !license_found || license.eq_ignore_ascii_case("UNLICENSED") {
        return Err(invalid(
            "目录包名与源码不符或缺少可检查许可证；未安装或启用",
        ));
    }
    let package = DshPackage {
        repository: repository.into(),
        commit: commit.into(),
        receipt: DshSourceReceipt {
            protocol: 1,
            name: name.into(),
            version: version.into(),
            entry: entry.into(),
            sdk_lock_sha256: sdk_lock_sha256.into(),
            files: receipt_files,
        },
    };
    package
        .verify(directory)
        .map_err(|e| invalid(&e.to_string()))?;
    control.check()?;
    Ok(PackageData {
        package,
        tree_sha: identity.tree.sha,
        objects: tree.tree,
        license: license.into(),
        declared_scripts: metadata
            .get("scripts")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    })
}
