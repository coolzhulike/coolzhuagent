//! Fusion 工作项确定性验收门（设计 §6.3）：
//! FilesExist / Command 合同由宿主执行，结果不以模型正文为准；
//! 门失败时主持不能 accept，失败证据随 rework 回流。
//! Command 门复用 goal_command 的真实宿主回执（超时/取消/进程回收同一实现）。
use super::*;

#[derive(Debug, Clone)]
pub(super) enum GateOutcome {
    /// 未声明验收合同（accept 由主持按验收标准自行审核）。
    NotRequired,
    Passed,
    Failed { reason: String, evidence: JsonValue },
    Invalid { reason: String },
}

impl GateOutcome {
    pub(super) fn passed(&self) -> bool {
        matches!(self, Self::NotRequired | Self::Passed)
    }
}

fn contract_kind(contract: &JsonValue) -> Option<&str> {
    contract
        .get("type")
        .or_else(|| contract.get("kind"))
        .and_then(JsonValue::as_str)
}

fn resolve_workspace_path(workspace: &Path, raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    let joined = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    // 防逃逸：最深存在祖先 canonicalize + 拼接剩余分量（Windows 下 canonicalize 返回
    // \\?\ 前缀，缺失文件直接字符串比较会误伤）。`..` 逃逸因祖先已归一而被拦下。
    let canonical_root = std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut ancestor = joined.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if ancestor.exists() {
            break;
        }
        match ancestor.file_name() {
            Some(name) => {
                tail.push(name.to_os_string());
                ancestor = ancestor.parent().map(Path::to_path_buf)?;
            }
            None => return None,
        }
    }
    let mut canonical = std::fs::canonicalize(&ancestor).unwrap_or(ancestor);
    for component in tail.iter().rev() {
        canonical.push(component);
    }
    if canonical.starts_with(&canonical_root) {
        Some(canonical)
    } else {
        None
    }
}

fn check_files_exist(contract: &JsonValue, workspace: &Path) -> GateOutcome {
    let Some(paths) = contract.get("paths").and_then(JsonValue::as_array) else {
        return GateOutcome::Invalid {
            reason: "FilesExist 合同缺少 paths 数组".to_string(),
        };
    };
    if paths.is_empty() {
        return GateOutcome::Invalid {
            reason: "FilesExist.paths 不能为空".to_string(),
        };
    }
    let mut missing = Vec::new();
    let mut rejected = Vec::new();
    for entry in paths {
        let Some(raw) = entry.as_str() else {
            return GateOutcome::Invalid {
                reason: "FilesExist.paths 必须全是字符串".to_string(),
            };
        };
        match resolve_workspace_path(workspace, raw) {
            Some(path) if path.is_file() => {
                let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if size == 0 {
                    missing.push(format!("{raw}（文件为空）"));
                }
            }
            Some(_) => missing.push(format!("{raw}（不是文件）")),
            None => rejected.push(raw.to_string()),
        }
    }
    if !rejected.is_empty() {
        return GateOutcome::Invalid {
            reason: format!("路径不在工作区内或非法：{}", rejected.join(", ")),
        };
    }
    if missing.is_empty() {
        GateOutcome::Passed
    } else {
        GateOutcome::Failed {
            reason: format!("缺失/空产物：{}", missing.join(", ")),
            evidence: json!({ "missing_artifacts": missing }),
        }
    }
}

/// 工作项 accept 前的确定性门；Command 逐条执行，首个失败即停。
pub(super) async fn evaluate(verification_json: Option<&str>, workspace: &Path) -> GateOutcome {
    let Some(raw) = verification_json else {
        return GateOutcome::NotRequired;
    };
    let contract: JsonValue = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(error) => {
            return GateOutcome::Invalid {
                reason: format!("验收合同不是合法 JSON：{error}"),
            };
        }
    };
    match contract_kind(&contract) {
        Some("FilesExist") => check_files_exist(&contract, workspace),
        Some("Command") => {
            let Some(commands) = contract.get("commands").and_then(JsonValue::as_array) else {
                return GateOutcome::Invalid {
                    reason: "Command 合同缺少 commands 数组".to_string(),
                };
            };
            if commands.is_empty()
                || commands
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
            {
                return GateOutcome::Invalid {
                    reason: "Command 合同必须包含非空命令".to_string(),
                };
            }
            let cwd = match contract.get("cwd") {
                None => workspace.to_path_buf(),
                Some(JsonValue::String(raw)) if raw.trim().is_empty() => workspace.to_path_buf(),
                Some(JsonValue::String(raw)) => match resolve_workspace_path(workspace, raw) {
                    Some(path) => path,
                    None => {
                        return GateOutcome::Invalid {
                            reason: format!("Command cwd 不在工作区内：{raw}"),
                        };
                    }
                },
                _ => {
                    return GateOutcome::Invalid {
                        reason: "Command cwd 必须是字符串".to_string(),
                    };
                }
            };
            let timeout_ms = match contract.get("timeout_ms") {
                None => 600_000,
                Some(value) => match value.as_u64().filter(|n| *n > 0) {
                    Some(n) => n,
                    None => {
                        return GateOutcome::Invalid {
                            reason: "Command timeout_ms 必须大于 0".to_string(),
                        };
                    }
                },
            };
            let mut receipts = Vec::new();
            for command in commands.iter().map(|v| v.as_str().unwrap()) {
                let receipt = goal_command::run_receipt(command, &cwd, timeout_ms).await;
                let failed = !matches!(receipt.status, goal_command::Status::Passed);
                receipts.push(json!({
                    "command": receipt.command,
                    "status": format!("{:?}", receipt.status).to_lowercase(),
                    "exit_code": receipt.exit_code,
                    "reason": receipt.reason,
                }));
                if failed {
                    return GateOutcome::Failed {
                        reason: receipt.reason.clone(),
                        evidence: json!({ "command_failures": receipts }),
                    };
                }
            }
            GateOutcome::Passed
        }
        Some(other) => GateOutcome::Invalid {
            reason: format!("未知验收类型 {other}（支持 FilesExist/Command）"),
        },
        None => GateOutcome::Invalid {
            reason: "验收合同缺少 type".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn files_exist_gate_reports_missing_and_rejects_escape() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("real.txt"), "content").unwrap();
        std::fs::write(root.path().join("empty.txt"), "").unwrap();
        let ok = json!({"type":"FilesExist","paths":["real.txt"]});
        assert!(evaluate(Some(&ok.to_string()), root.path()).await.passed());
        let missing = json!({"type":"FilesExist","paths":["real.txt","nope.txt","empty.txt"]});
        match evaluate(Some(&missing.to_string()), root.path()).await {
            GateOutcome::Failed { reason, evidence } => {
                assert!(reason.contains("nope.txt"));
                assert!(evidence["missing_artifacts"].as_array().unwrap().len() == 2);
            }
            other => panic!("missing artifacts must fail the gate: {other:?}"),
        }
        let escape = json!({"type":"FilesExist","paths":["../outside.txt"]});
        assert!(matches!(
            evaluate(Some(&escape.to_string()), root.path()).await,
            GateOutcome::Invalid { .. }
        ));
    }

    #[tokio::test]
    async fn command_gate_runs_host_receipts_and_stops_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let ok = json!({"type":"Command","commands":["exit 0"],"timeout_ms":90_000});
        assert!(evaluate(Some(&ok.to_string()), root.path()).await.passed());
        let fail = json!({"type":"Command","commands":["exit 0","exit 7","exit 0"]});
        match evaluate(Some(&fail.to_string()), root.path()).await {
            GateOutcome::Failed { evidence, .. } => {
                let receipts = evidence["command_failures"].as_array().unwrap();
                assert_eq!(receipts.len(), 2, "首个失败后不再执行后续命令");
                assert_eq!(receipts[1]["exit_code"].as_i64(), Some(7));
            }
            other => panic!("failing command must fail the gate: {other:?}"),
        }
        let bad = json!({"type":"Command","commands":[]});
        assert!(matches!(
            evaluate(Some(&bad.to_string()), root.path()).await,
            GateOutcome::Invalid { .. }
        ));
        let unknown = json!({"type":"SpellCheck"});
        assert!(matches!(
            evaluate(Some(&unknown.to_string()), root.path()).await,
            GateOutcome::Invalid { .. }
        ));
        assert!(evaluate(None, root.path()).await.passed());
    }
}
