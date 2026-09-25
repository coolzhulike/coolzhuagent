//! Git Workflow Plugin — atomic commits, Gerrit Change-Id injection.
//!
//! Provides helper functions for:
//! - Validating commit messages (Change-Id presence, length)
//! - Auto-generating Gerrit Change-Id footers
//! - Building Gerrit push commands with reviewer support

// ---------------------------------------------------------------------------
// Commit validation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitValidation {
    Ok,
    NoChangeId,
    EmptyMessage,
    MessageTooLong(usize),
}

/// Validate a commit message for Gerrit compliance.
pub fn validate_commit_message(msg: &str) -> CommitValidation {
    let trimmed = msg.trim();
    if trimmed.is_empty() {
        return CommitValidation::EmptyMessage;
    }
    if trimmed.len() > 500 {
        return CommitValidation::MessageTooLong(trimmed.len());
    }
    let has_change_id = trimmed
        .lines()
        .any(|line| line.trim_start().starts_with("Change-Id:"));
    if !has_change_id {
        return CommitValidation::NoChangeId;
    }
    CommitValidation::Ok
}

// ---------------------------------------------------------------------------
// Change-Id generation
// ---------------------------------------------------------------------------

/// Generate a Gerrit-style Change-Id footer.
///
/// Real implementation should use `git commit` with a `commit-msg` hook
/// that computes `SHA1("tree <tree>\nparent <parent>\nauthor ...\ncommitter ...\n\n<msg>")`.
/// For testing, we use a simple random ID.
pub fn generate_change_id() -> String {
    let mut buf = [0u8; 20];
    getrandom::fill(&mut buf).unwrap_or_default();
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    format!("I{hex}")
}

/// Append a Change-Id footer to a commit message.
pub fn append_change_id(msg: &str) -> String {
    let change_id = generate_change_id();
    format!("{msg}\n\nChange-Id: {change_id}")
}

// ---------------------------------------------------------------------------
// Gerrit push targets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GerritPushTarget {
    pub branch: String,
    pub change_id: Option<String>,
    pub reviewers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushCommand {
    Review {
        target: String,
        change_id: Option<String>,
    },
    Draft {
        target: String,
    },
    Direct {
        target: String,
    },
}

/// Build a Gerrit push refspec from a push target.
pub fn build_gerrit_push_command(target: &GerritPushTarget) -> PushCommand {
    let base = format!("HEAD:refs/for/{}", target.branch);
    if target.reviewers.is_empty() {
        PushCommand::Review {
            target: base,
            change_id: target.change_id.clone(),
        }
    } else {
        let reviewers = target.reviewers.join(",");
        PushCommand::Review {
            target: format!("{base}%r={reviewers}"),
            change_id: target.change_id.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Commit message formatting
// ---------------------------------------------------------------------------

/// Format a commit message with auto-appended Change-Id.
pub fn format_commit_body(body: &str, _branch: &str) -> String {
    append_change_id(body)
}

/// Summarize changed files for a commit message.
pub fn commit_files_summary(files: &[String]) -> String {
    match files.len() {
        0 => String::new(),
        1 => files[0].clone(),
        n => format!("{} and {} more file(s)", files[0], n - 1),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_empty_message() {
        assert_eq!(validate_commit_message(""), CommitValidation::EmptyMessage);
        assert_eq!(
            validate_commit_message("   \n  "),
            CommitValidation::EmptyMessage
        );
    }

    #[test]
    fn validate_no_change_id() {
        assert_eq!(
            validate_commit_message("fix: update handler"),
            CommitValidation::NoChangeId
        );
    }

    #[test]
    fn validate_valid() {
        let msg = "feat: add plugin\n\nChange-Id: Iabc123\n";
        assert_eq!(validate_commit_message(msg), CommitValidation::Ok);
    }

    #[test]
    fn validate_too_long() {
        let long = "a".repeat(600);
        assert!(matches!(
            validate_commit_message(&long),
            CommitValidation::MessageTooLong(600)
        ));
    }

    #[test]
    fn append_change_id_adds_footer() {
        let result = append_change_id("fix: bug");
        assert!(result.starts_with("fix: bug"));
        assert!(result.contains("Change-Id: I"));
    }

    #[test]
    fn push_to_main_without_reviewers() {
        let target = GerritPushTarget {
            branch: "main".into(),
            change_id: Some("Iabc".into()),
            reviewers: vec![],
        };
        match build_gerrit_push_command(&target) {
            PushCommand::Review { target, .. } => {
                assert_eq!(target, "HEAD:refs/for/main");
            }
            _ => panic!("expected Review"),
        }
    }

    #[test]
    fn push_with_reviewers() {
        let target = GerritPushTarget {
            branch: "main".into(),
            change_id: None,
            reviewers: vec!["r1@e.com".into(), "r2@e.com".into()],
        };
        match build_gerrit_push_command(&target) {
            PushCommand::Review { target, .. } => {
                assert!(target.contains("r=r1@e.com"), "should include reviewers");
                assert!(target.contains("r2@e.com"), "should include both");
            }
            _ => panic!("expected Review"),
        }
    }

    #[test]
    fn commit_files_single() {
        assert_eq!(commit_files_summary(&["src/main.rs".into()]), "src/main.rs");
    }

    #[test]
    fn commit_files_multiple() {
        let s = commit_files_summary(&["a.rs".into(), "b.rs".into(), "c.toml".into()]);
        assert!(s.contains("a.rs"));
        assert!(s.contains("2 more"));
    }

    #[test]
    fn change_id_format() {
        let id = generate_change_id();
        assert!(id.starts_with('I'), "Change-Id must start with I");
        assert!(id.len() == 41, "Change-Id should be 41 chars (I + 40 hex)");
    }

    #[test]
    fn format_commit_body_appends_change_id() {
        let result = format_commit_body("feat: add plugin", "main");
        assert!(result.contains("feat: add plugin"));
        assert!(result.contains("Change-Id: I"));
    }

    #[test]
    fn push_to_feature_branch() {
        let target = GerritPushTarget {
            branch: "feature/tdd".into(),
            change_id: None,
            reviewers: vec![],
        };
        match build_gerrit_push_command(&target) {
            PushCommand::Review { target, .. } => {
                assert_eq!(target, "HEAD:refs/for/feature/tdd");
            }
            _ => panic!("expected feature branch push"),
        }
    }
}
