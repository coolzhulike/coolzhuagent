//! 社区条目解析为一次固定来源；目录、源码身份与可执行资格分别核验。
use crate::dsh_source_download::{self, DownloadControl, DownloadError, PreparedPackage};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SourceCandidate {
    repository: String,
    expected_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResolvedSource {
    pub repository: String,
    pub commit: String,
    pub expected_name: String,
}

fn invalid(message: &str) -> DownloadError {
    DownloadError {
        code: "source_invalid",
        message: message.into(),
        // 解析本身没有创建目录；下载失败的清理状态仍由下载器决定。
        cleanup_confirmed: false,
    }
}

fn package_name(value: &str) -> bool {
    let part = |v: &str| {
        !v.is_empty()
            && !v.starts_with(['.', '_'])
            && v.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_.".contains(&b))
    };
    if value.len() > 214 {
        return false;
    }
    match value.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| part(scope) && part(name)),
        None => part(value),
    }
}

fn commit_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl SourceCandidate {
    pub fn new(owner: &str, repository: &str, npm: Option<&str>) -> Result<Self, DownloadError> {
        // 只规整 GitHub 根仓库常用的尾部形式；分支路径、凭据及查询参数仍拒绝。
        let repository = repository.trim_end_matches('/');
        let repository = repository.strip_suffix(".git").unwrap_or(repository);
        let (actual_owner, _) = dsh_source_download::repository_parts(repository)?;
        if actual_owner != owner {
            return Err(invalid("目录作者与仓库所有者不一致，未解析来源"));
        }
        if npm.is_some_and(|name| !package_name(name)) {
            return Err(invalid(
                "目录 npm 字段包含无效包名或安装 spec，未猜测同名包",
            ));
        }
        // Git 来源的目录允许 npm=null；包名随后从固定提交根清单读取并由完整下载核验。
        Ok(Self {
            repository: repository.into(),
            expected_name: npm.map(str::to_owned),
        })
    }
}

#[derive(Deserialize)]
struct RepositoryOwner {
    login: String,
}
#[derive(Deserialize)]
struct RepositoryIdentity {
    name: String,
    full_name: String,
    html_url: String,
    owner: RepositoryOwner,
    private: bool,
}
#[derive(Deserialize)]
struct CommitIdentity {
    sha: String,
}

fn verify_repository(candidate: &SourceCandidate, bytes: &[u8]) -> Result<(), DownloadError> {
    let identity: RepositoryIdentity =
        serde_json::from_slice(bytes).map_err(|_| invalid("仓库身份响应无效"))?;
    let (owner, repo) = dsh_source_download::repository_parts(&candidate.repository)?;
    if identity.private
        || !identity.owner.login.eq_ignore_ascii_case(owner)
        || !identity.name.eq_ignore_ascii_case(repo)
        || !identity
            .full_name
            .eq_ignore_ascii_case(&format!("{owner}/{repo}"))
        || !identity
            .html_url
            .eq_ignore_ascii_case(&candidate.repository)
    {
        return Err(invalid("仓库作者、名称或公开来源身份不符，未改用其它地址"));
    }
    Ok(())
}

/// HEAD 只解析一次，后续所有读取均使用返回的固定 commit；从不按目录版本猜 npm 发行包。
pub async fn resolve(
    candidate: &SourceCandidate,
    requested_commit: Option<&str>,
    control: &DownloadControl,
) -> Result<ResolvedSource, DownloadError> {
    control.check()?;
    if requested_commit.is_some_and(|value| !commit_sha(value)) {
        return Err(invalid(
            "修订必须是40位固定 commit，不能提交分支、标签或安装命令",
        ));
    }
    let (owner, repo) = dsh_source_download::repository_parts(&candidate.repository)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .user_agent("coolzhu-web-console/DSH-source-resolution")
        .build()
        .map_err(|_| invalid("无法创建固定来源连接"))?;
    let api = format!("https://api.github.com/repos/{owner}/{repo}");
    let bytes = dsh_source_download::get(&client, api.parse().unwrap(), 64 * 1024, control).await?;
    verify_repository(candidate, &bytes)?;
    let commit = match requested_commit {
        Some(value) => value.to_owned(),
        None => {
            let bytes = dsh_source_download::get(
                &client,
                format!("{api}/commits/HEAD").parse().unwrap(),
                512 * 1024,
                control,
            )
            .await?;
            let identity: CommitIdentity =
                serde_json::from_slice(&bytes).map_err(|_| invalid("默认修订响应无效"))?;
            if !commit_sha(&identity.sha) {
                return Err(invalid("默认修订未解析成固定 commit"));
            }
            identity.sha
        }
    };
    let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/{commit}/package.json");
    let bytes = dsh_source_download::get(&client, url.parse().unwrap(), 64 * 1024, control).await?;
    let package: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("固定提交根目录的 package.json 无效"))?;
    let name = package["name"]
        .as_str()
        .filter(|name| package_name(name))
        .ok_or_else(|| invalid("固定提交根清单缺少明确包名，未猜测 registry 发行包"))?;
    if candidate
        .expected_name
        .as_deref()
        .is_some_and(|expected| expected != name)
    {
        return Err(invalid("目录 npm 包名与固定提交根清单不一致"));
    }
    control.check()?;
    Ok(ResolvedSource {
        repository: candidate.repository.clone(),
        commit,
        expected_name: name.into(),
    })
}

/// 同一次预算贯穿作者核验、修订解析和完整源码下载；调用者决定暂存是否交给已有安装事务。
pub async fn prepare(
    candidate: &SourceCandidate,
    requested_commit: Option<&str>,
    sdk_lock_sha256: &str,
    temporary_parent: &Path,
    control: &DownloadControl,
) -> Result<PreparedPackage, DownloadError> {
    let source = resolve(candidate, requested_commit, control).await?;
    dsh_source_download::download(
        &source.repository,
        &source.commit,
        &source.expected_name,
        sdk_lock_sha256,
        temporary_parent,
        control,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_identity_does_not_authorize_arbitrary_install_specs() {
        let repo = "https://github.com/omdsh-dev/dsh-tool-calculator";
        for name in [
            Some("calculator@latest"),
            Some("git+https://other"),
            Some("../calculator"),
            Some("@scope/one/two"),
            Some("@scope/.hidden"),
        ] {
            assert!(SourceCandidate::new("omdsh-dev", repo, name).is_err());
        }
        assert!(SourceCandidate::new("other", repo, Some("calculator")).is_err());
        assert!(SourceCandidate::new("omdsh-dev", repo, None).is_ok());
        assert!(SourceCandidate::new(
            "omdsh-dev",
            &format!("{repo}/tree/main"),
            Some("calculator")
        )
        .is_err());
        assert!(SourceCandidate::new(
            "omdsh-dev",
            &format!("{repo}.git/"),
            Some("@deepseek-ai/dsh-tool-calculator")
        )
        .is_ok());
    }

    #[test]
    fn repository_response_cannot_substitute_another_source() {
        let candidate =
            SourceCandidate::new("owner", "https://github.com/owner/repo", Some("calculator"))
                .unwrap();
        let mut identity = serde_json::json!({"name":"repo", "full_name":"owner/repo",
            "html_url":"https://github.com/owner/repo", "owner":{"login":"owner"}, "private":false});
        assert!(verify_repository(&candidate, &serde_json::to_vec(&identity).unwrap()).is_ok());
        for (field, value) in [
            ("full_name", serde_json::json!("other/repo")),
            (
                "html_url",
                serde_json::json!("https://example.com/owner/repo"),
            ),
            ("private", serde_json::json!(true)),
        ] {
            let before = identity[field].clone();
            identity[field] = value;
            assert!(
                verify_repository(&candidate, &serde_json::to_vec(&identity).unwrap()).is_err()
            );
            identity[field] = before;
        }
    }
}
