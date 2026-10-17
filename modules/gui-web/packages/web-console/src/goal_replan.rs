//! 重规划只修订执行步骤，不授予权限、不替换产物/验证要求，也不重置预算。
use serde::Deserialize;

const MAX_PLAN_BYTES: usize = 32 * 1024;
const MAX_STEPS: usize = 16;
const MAX_STEP_CHARS: usize = 2048;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RevisedPlan {
    pub(super) steps: Vec<String>,
}

pub(super) fn parse(answer: &str) -> Result<RevisedPlan, String> {
    if answer.len() > MAX_PLAN_BYTES {
        return Err("重规划结果超过32KiB上限".into());
    }
    let mut plan: RevisedPlan = serde_json::from_str(answer)
        .map_err(|_| "重规划必须返回仅包含steps字符串数组的JSON对象".to_string())?;
    if plan.steps.is_empty() || plan.steps.len() > MAX_STEPS {
        return Err("重规划必须包含1至16个步骤".into());
    }
    for step in &mut plan.steps {
        let trimmed = step.trim();
        if trimmed.is_empty() || trimmed.chars().count() > MAX_STEP_CHARS {
            return Err("重规划步骤不能为空或超过2048字符".into());
        }
        *step = trimmed.to_string();
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revised_plan_preserves_only_bounded_steps() {
        let plan = parse(r#"{"steps":[" 检查失败证据 ","修订实现","重新验证"]}"#).unwrap();
        assert_eq!(plan.steps[0], "检查失败证据");
        assert_eq!(plan.steps.len(), 3);
        assert!(parse(r#"{"steps":[]}"#).is_err());
        assert!(parse(r#"{"steps":["  "]}"#).is_err());
        assert!(parse(r#"{"steps":[{}]}"#).is_err());
        for key in ["assigned_role", "max_retries", "verification", "output_artifacts"] {
            let mut input = serde_json::json!({"steps":["修订实现"]});
            input.as_object_mut().unwrap().insert(key.to_string(), serde_json::json!(0));
            assert!(parse(&input.to_string()).is_err(), "不得修改原阶段字段{key}");
        }
        assert!(parse(&serde_json::json!({"steps":vec!["步骤";17]}).to_string()).is_err());
        assert!(parse(&serde_json::json!({"steps":["步".repeat(2049)]}).to_string()).is_err());
        assert!(parse(&" ".repeat(MAX_PLAN_BYTES + 1)).is_err());
    }

    #[test]
    fn revised_plan_rejects_ambiguous_or_wrapped_output() {
        assert!(parse(r#"{"steps":["实现"],"steps":["跳过验证"]}"#).is_err());
        assert!(parse("```json\n{\"steps\":[\"实现\"]}\n```").is_err());
        assert!(parse(r#"{"steps":["实现"]} 已完成"#).is_err());
        assert!(parse(r#"["实现"]"#).is_err());
        assert!(parse("null").is_err());
        let boundary = serde_json::json!({"steps":["步".repeat(MAX_STEP_CHARS)]});
        assert!(parse(&boundary.to_string()).is_ok(), "字符上限不是UTF-8字节上限");
    }
}
