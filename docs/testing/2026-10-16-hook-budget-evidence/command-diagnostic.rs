fn main() {
    let authorization = runtime::PermissionPolicy::new(runtime::PermissionMode::DangerFullAccess)
        .with_tool_requirement(runtime::HOOK_CAPABILITY_PRE_TOOL_USE, runtime::PermissionMode::DangerFullAccess);
    for command in [
        "echo quoted>\"D:\\coolzhuagent\\tmp\\goal-32wbs-acceptance-20261013\\hook-budget-20261016\\quoted.marker\"",
        "echo unquoted>D:\\coolzhuagent\\tmp\\goal-32wbs-acceptance-20261013\\hook-budget-20261016\\unquoted.marker",
        "powershell.exe -NoProfile -NonInteractive -File \"D:\\coolzhuagent\\tmp\\goal-32wbs-acceptance-20261013\\hook-budget-20261016\\.tmp3tgf4R\\bounded-hook.ps1\"",
    ] {
        let runner = runtime::HookRunner::new(runtime::RuntimeHookConfig::new(vec![command.to_owned()], Vec::new()))
            .with_authorization(authorization.clone());
        let result = runner.run_pre_tool_use("read_file", "{}");
        println!("{result:?}");
    }
}
