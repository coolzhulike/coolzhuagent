//! Command 验证结果不能以空失败列表表示成功；回执只来自宿主执行，不反序列化模型正文。
use super::*;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub(super) enum State { Passed, Failed, NotRequired, Skipped, Invalid, Unable }

#[derive(Clone, Debug, Serialize)]
pub(super) struct Result {
    pub state: State,
    pub reason: String,
    contract: Option<JsonValue>,
    pub receipts: Vec<goal_command::Receipt>,
    legacy_failures: Option<Vec<String>>,
}
impl Result {
    pub fn state(state:State, reason:impl Into<String>)->Self {
        Self {state,reason:reason.into(),contract:None,receipts:vec![],legacy_failures:None}
    }
    pub fn unable(reason:impl Into<String>)->Self {Self::state(State::Unable,reason)}
    pub fn not_required(reason:impl Into<String>)->Self {Self::state(State::NotRequired,reason)}
    pub fn force_unable(&mut self,reason:impl Into<String>) {self.state=State::Unable;self.reason=reason.into();}
    /// 旧空列表只表示无回执。只有非 Command 合同才可不适用，不能升级为 Command 成功。
    pub fn blockers(&self,contract:Option<&JsonValue>,planning:bool)->Vec<String> {
        if let Some(failures)=&self.legacy_failures {
            if !failures.is_empty(){return failures.clone();}
            if !is_command(contract)||planning{return vec![];}
            return vec!["Command 验证 unable：旧结果没有可信执行回执".into()];
        }
        if planning && self.state==State::NotRequired {return vec![];}
        if self.state==State::NotRequired && !is_command(contract){return vec![];}
        if self.state==State::Passed && self.contract.as_ref()==contract && !self.receipts.is_empty()
            && self.receipts.iter().all(|r|r.status==goal_command::Status::Passed && r.exit_code==Some(0)) {return vec![];}
        vec![format!("Command 验证 {:?}：{}",self.state,if self.reason.is_empty(){"回执缺失或合同已变化"}else{&self.reason})]
    }
}
impl From<Vec<String>> for Result {
    fn from(failures:Vec<String>)->Self {Self {legacy_failures:Some(failures),..Self::unable("历史结果没有结构化回执")}}
}
pub(super) fn is_command(contract:Option<&JsonValue>)->bool {
    contract.and_then(|v|v.get("type").or_else(||v.get("kind"))).and_then(JsonValue::as_str)==Some("Command")
}
pub(super) async fn evaluate(contract:Option<&JsonValue>,enabled:bool,workspace:&Path)->Result {
    evaluate_with_receipts(contract,enabled,workspace,&[]).await
}

/// 模型诊断已经产生真实宿主回执时，按命令逐条消费；只有缺失/不匹配才启动 runtime。
pub(super) async fn evaluate_with_receipts(
    contract:Option<&JsonValue>,enabled:bool,workspace:&Path,tool_messages:&[ChatMessageDto],
)->Result {
    let mut prior=Vec::new();
    evaluate_with_prior(contract,enabled,workspace,tool_messages,&mut prior).await
}

/// `prior` 只接收同阶段 repair_result 中由宿主保存的结构化回执；旧/无 shell 的记录不匹配。
pub(super) async fn evaluate_with_prior(
    contract:Option<&JsonValue>,enabled:bool,workspace:&Path,tool_messages:&[ChatMessageDto],
    prior:&mut Vec<goal_command::Receipt>,
)->Result {
    if contract.is_some_and(|v|v.get("type").or_else(||v.get("kind")).and_then(JsonValue::as_str).is_none()) {
        return Result::state(State::Invalid,"验证合同缺少明确类型，不能视为未要求验证");
    }
    if !is_command(contract){return Result::not_required("当前合同未要求 Command；其他验收仍需独立判断");}
    let contract=contract.unwrap();
    let Some(values)=contract.get("commands").and_then(JsonValue::as_array) else {
        return Result::state(State::Invalid,"Command 合同缺少 commands 数组");
    };
    if values.is_empty()||values.iter().any(|v|v.as_str().is_none_or(|s|s.trim().is_empty())) {
        return Result::state(State::Invalid,"Command 合同必须包含非空命令，不能默默过滤无效项");
    }
    if !enabled{return Result::state(State::Skipped,"明确要求的 Command 门已禁用，合同尚未满足");}
    let cwd=match contract.get("cwd") {
        // 归一化器会为缺省 cwd 持久化空串；空串与缺省同义，都回落到工作区根。
        None=>workspace.to_path_buf(),
        Some(JsonValue::String(s))=>{
            let s=s.trim();
            if s.is_empty(){workspace.to_path_buf()}else{let p=PathBuf::from(s);if p.is_absolute(){p}else{workspace.join(p)}}
        }
        _=>return Result::state(State::Invalid,"Command cwd 必须是非空路径"),
    };
    let cwd=std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let timeout=match contract.get("timeout_ms") {
        None=>600_000,
        Some(value)=>match value.as_u64().filter(|n|*n>0){Some(n)=>n,None=>return Result::state(State::Invalid,"Command timeout_ms 必须大于0")},
    };
    let mut result=Result {contract:Some(contract.clone()),..Result::state(State::Passed,"")};
    for command in values.iter().map(|v|v.as_str().unwrap()) {
        if let Some(receipt)=take_matching_receipt(command,&cwd,timeout,tool_messages,prior) {
            result.receipts.push(receipt);
            continue;
        }
        let receipt=goal_command::run_receipt(command,&cwd,timeout).await;
        let failed=receipt.status!=goal_command::Status::Passed;
        if failed {result.state=if receipt.status==goal_command::Status::Failed{State::Failed}else{State::Unable};result.reason=receipt.reason.clone();}
        result.receipts.push(receipt);
        // 第一条未满足后停止后续命令，取消/超时不再发起新的进程。
        if failed{break;}
    }
    result
}

fn take_matching_receipt(
    command:&str,cwd:&Path,timeout:u64,tool_messages:&[ChatMessageDto],prior:&mut Vec<goal_command::Receipt>,
)->Option<goal_command::Receipt> {
    let expected_shell=goal_command::shell_path();
    let current_digest=goal_command::verification_digest(cwd);
    for message in tool_messages {
        let Some(receipt)=goal_command::take_diagnostic_receipt(&message.id) else {continue;};
        let Ok(actual_cwd)=std::fs::canonicalize(&receipt.cwd) else {continue;};
        if receipt.command!=command
            || actual_cwd!=cwd
            || !receipt.shell.eq_ignore_ascii_case(&expected_shell)
            || receipt.interrupted
            || receipt.exit_code!=Some(0)
            || (timeout>0 && receipt.timeout_ms>0 && receipt.timeout_ms<timeout)
        {continue;}
        return Some(goal_command::Receipt {
            id:receipt.id,
            command:receipt.command,
            cwd:receipt.cwd,
            shell:receipt.shell,
            timeout_ms:receipt.timeout_ms,
            started_at_unix_ms:receipt.started_at_unix_ms,
            finished_at_unix_ms:receipt.finished_at_unix_ms,
            status:goal_command::Status::Passed,
            exit_code:Some(0),
            reason:"复用模型诊断的真实宿主回执".into(),
            input_digest:current_digest.clone(),
        });
    }
    let index=prior.iter().position(|receipt| {
        if receipt.status!=goal_command::Status::Passed || receipt.exit_code!=Some(0) {return false;}
        let Ok(actual_cwd)=std::fs::canonicalize(&receipt.cwd) else {return false;};
        receipt.command==command && actual_cwd==cwd
            && receipt.shell.eq_ignore_ascii_case(&expected_shell)
            && receipt.input_digest==current_digest
            && (timeout==0 || receipt.timeout_ms==0 || receipt.timeout_ms>=timeout)
    })?;
    Some(prior.remove(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn non_execution_states_and_legacy_receipts_cannot_pass_command() {
        let root=tempfile::tempdir().unwrap();let command=json!({"type":"Command","commands":["exit 0"]});
        let absent=evaluate(None,false,root.path()).await;assert_eq!(absent.state,State::NotRequired);
        let files=json!({"type":"FilesExist","paths":["artifact.txt"]});
        assert_eq!(evaluate(Some(&files),false,root.path()).await.state,State::NotRequired);
        assert_eq!(evaluate(Some(&command),false,root.path()).await.state,State::Skipped);
        for commands in [json!([]),json!([""]),json!([3]),json!(["exit 0",null])]{
            let bad=json!({"type":"Command","commands":commands});
            let r=evaluate(Some(&bad),true,root.path()).await;assert_eq!(r.state,State::Invalid);assert!(!r.blockers(Some(&bad),false).is_empty());
        }
        let legacy=Result::from(Vec::<String>::new());assert!(!legacy.blockers(Some(&command),false).is_empty());assert!(legacy.blockers(Some(&files),false).is_empty());
        assert!(!absent.blockers(Some(&command),false).is_empty());
    }
    #[tokio::test]
    async fn input_digest_reuses_unchanged_and_invalidates_modified_source() {
        let root=tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), "pub fn a() {}").unwrap();
        let contract=json!({"type":"Command","commands":["exit 0"],"timeout_ms":90_000});
        let first=evaluate(Some(&contract),true,root.path()).await;
        assert_eq!(first.state,State::Passed);assert!(!first.receipts[0].input_digest.is_empty());
        let mut prior=first.receipts.clone();
        let reused=evaluate_with_prior(Some(&contract),true,root.path(),&[],&mut prior).await;
        assert_eq!(reused.receipts[0].id,first.receipts[0].id,"无变化必须复用原回执");
        assert!(prior.is_empty());

        std::fs::write(root.path().join("lib.rs"), "pub fn a() { /* changed */ }").unwrap();
        let mut stale=first.receipts.clone();
        let rerun=evaluate_with_prior(Some(&contract),true,root.path(),&[],&mut stale).await;
        assert_eq!(rerun.state,State::Passed);
        assert_ne!(rerun.receipts[0].id,first.receipts[0].id,"源码变化后旧回执必须失效");
        assert_eq!(stale.len(),1,"不匹配的旧回执不能被消费");
    }

    #[tokio::test]
    async fn real_receipts_and_contract_changes_control_consumption() {
        let root=tempfile::tempdir().unwrap();let command=json!({"type":"Command","commands":["exit 0"],"timeout_ms":90_000});
        let r=evaluate(Some(&command),true,root.path()).await;
        assert_eq!(r.state,State::Passed,"{r:?}");assert_eq!(r.receipts.len(),1);assert_eq!(r.receipts[0].exit_code,Some(0));
        assert!(r.blockers(Some(&command),false).is_empty());
        let other=json!({"type":"Command","commands":["exit 1"]});assert!(!r.blockers(Some(&other),false).is_empty());
        let failed=evaluate(Some(&other),true,root.path()).await;assert_eq!(failed.state,State::Failed);assert_eq!(failed.receipts[0].exit_code,Some(1));
        assert_ne!(r.receipts[0].id,failed.receipts[0].id);
        let invalid_cwd=json!({"type":"Command","commands":["exit 0"],"cwd":root.path().join("missing")});
        let unstarted=evaluate(Some(&invalid_cwd),true,root.path()).await;
        assert_eq!(unstarted.state,State::Unable);
        // 归一化器会把缺省 cwd 存成空串；空串 cwd 必须等价于缺省（工作区根），不能判 Invalid。
        let empty_cwd=json!({"type":"Command","commands":["exit 0"],"cwd":""});
        assert_eq!(evaluate(Some(&empty_cwd),true,root.path()).await.state,State::Passed);
        let bad_cwd_type=json!({"type":"Command","commands":["exit 0"],"cwd":7});
        assert_eq!(evaluate(Some(&bad_cwd_type),true,root.path()).await.state,State::Invalid);
        if let Some(dir)=std::env::var_os("COOLZHU_HARNESS_EVIDENCE_DIR") {
            std::fs::write(PathBuf::from(dir).join("ha09-command-receipts.json"),serde_json::to_vec_pretty(&json!({"passed":r,"failed":failed,"unable":unstarted})).unwrap()).unwrap();
        }
    }
}
