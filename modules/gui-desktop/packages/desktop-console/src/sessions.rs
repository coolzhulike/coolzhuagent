use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use runtime::Session;

pub const MAX_MANAGED_SESSIONS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiSessionHandle {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiManagedSessionSummary {
    pub id: String,
    pub path: PathBuf,
    pub modified_epoch_secs: u64,
}

pub fn sessions_dir(workspace: &Path, profile: &str) -> Result<PathBuf, String> {
    let mut path = workspace.join(".claw").join("sessions");
    let trimmed = profile.trim();
    if !trimmed.is_empty() && trimmed != "default" {
        path = path.join(trimmed);
    }
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create sessions dir {}: {error}", path.display()))?;
    Ok(path)
}

pub fn create_managed_session_handle(
    workspace: &Path,
    profile: &str,
) -> Result<GuiSessionHandle, String> {
    let id = generate_session_id();
    let path = sessions_dir(workspace, profile)?.join(format!("{id}.json"));
    Ok(GuiSessionHandle { id, path })
}

pub fn save_session(handle: &GuiSessionHandle, session: &Session) -> Result<(), String> {
    if let Some(parent) = handle.path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create session directory {}: {error}",
                parent.display()
            )
        })?;
    }
    session
        .save_to_path(&handle.path)
        .map_err(|error| format!("failed to save session {}: {error}", handle.path.display()))
}

pub fn load_session(handle: &GuiSessionHandle) -> Result<Session, String> {
    Session::load_from_path(&handle.path)
        .map_err(|error| format!("failed to load session {}: {error}", handle.path.display()))
}

pub fn delete_session(handle: &GuiSessionHandle) -> Result<(), String> {
    if handle.path.exists() {
        fs::remove_file(&handle.path).map_err(|error| {
            format!(
                "failed to delete session {}: {error}",
                handle.path.display()
            )
        })
    } else {
        Ok(())
    }
}

pub fn rename_session(
    handle: &GuiSessionHandle,
    new_name: &str,
) -> Result<GuiSessionHandle, String> {
    let sanitized = sanitize_session_name(new_name)?;
    let new_path = handle
        .path
        .parent()
        .ok_or_else(|| {
            format!(
                "failed to resolve session directory for {}",
                handle.path.display()
            )
        })?
        .join(format!("{sanitized}.json"));

    if new_path == handle.path {
        return Ok(handle.clone());
    }

    if new_path.exists() {
        return Err(format!("session name already exists: {sanitized}"));
    }

    fs::rename(&handle.path, &new_path).map_err(|error| {
        format!(
            "failed to rename session {} -> {}: {error}",
            handle.path.display(),
            new_path.display()
        )
    })?;

    Ok(GuiSessionHandle {
        id: sanitized,
        path: new_path,
    })
}

pub fn list_managed_sessions(
    workspace: &Path,
    profile: &str,
) -> Result<Vec<GuiManagedSessionSummary>, String> {
    let mut sessions = Vec::new();
    for entry in fs::read_dir(sessions_dir(workspace, profile)?)
        .map_err(|error| format!("failed to read session directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("failed to inspect session entry: {error}"))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let metadata = entry
            .metadata()
            .map_err(|error| format!("failed to read metadata {}: {error}", path.display()))?;
        let modified_epoch_secs = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
        sessions.push(GuiManagedSessionSummary {
            id,
            path,
            modified_epoch_secs,
        });
    }

    sessions.sort_by(|left, right| right.modified_epoch_secs.cmp(&left.modified_epoch_secs));
    Ok(sessions)
}

pub fn prune_managed_sessions(
    workspace: &Path,
    profile: &str,
    keep_id: Option<&str>,
) -> Result<(), String> {
    let mut sessions = list_managed_sessions(workspace, profile)?;
    if sessions.len() <= MAX_MANAGED_SESSIONS {
        return Ok(());
    }

    let mut keep_ids = HashSet::new();
    if let Some(keep_id) = keep_id {
        if sessions.iter().any(|session| session.id == keep_id) {
            keep_ids.insert(keep_id.to_string());
        }
    }

    let keep_budget = MAX_MANAGED_SESSIONS.saturating_sub(keep_ids.len());
    for session in sessions.iter().take(keep_budget) {
        keep_ids.insert(session.id.clone());
    }

    for session in sessions.drain(..) {
        if keep_ids.contains(&session.id) {
            continue;
        }
        fs::remove_file(&session.path).map_err(|error| {
            format!(
                "failed to prune session {}: {error}",
                session.path.display()
            )
        })?;
    }

    Ok(())
}

fn generate_session_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("session-{millis}")
}

fn sanitize_session_name(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return Err("session name cannot be empty".to_string());
    }

    let mut sanitized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        let mapped = match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ if ch.is_control() => '-',
            _ => ch,
        };
        sanitized.push(mapped);
    }

    let sanitized = sanitized.trim().trim_end_matches(['.', ' ']).to_string();
    if sanitized.is_empty() {
        return Err("session name cannot be empty".to_string());
    }

    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::{
        create_managed_session_handle, delete_session, list_managed_sessions, load_session,
        prune_managed_sessions, rename_session, save_session, sessions_dir, MAX_MANAGED_SESSIONS,
    };
    use runtime::ConversationMessage;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("claw-gui-session-tests-{nanos}"))
    }

    #[test]
    fn creates_lists_and_deletes_managed_sessions() {
        let workspace = temp_workspace();
        let profile = "default";
        fs::create_dir_all(&workspace).expect("workspace should create");

        let handle =
            create_managed_session_handle(&workspace, profile).expect("handle should create");
        let mut session = runtime::Session::new();
        session
            .messages
            .push(ConversationMessage::user_text("hello session"));
        save_session(&handle, &session).expect("session should save");

        let listed = list_managed_sessions(&workspace, profile).expect("sessions should list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, handle.id);

        let loaded = load_session(&handle).expect("session should load");
        assert_eq!(loaded.messages.len(), 1);

        delete_session(&handle).expect("session should delete");
        let listed_after_delete =
            list_managed_sessions(&workspace, profile).expect("sessions should list after delete");
        assert!(listed_after_delete.is_empty());

        fs::remove_dir_all(&workspace).expect("workspace should clean");
    }

    #[test]
    fn profile_specific_sessions_use_subdirectory() {
        let workspace = temp_workspace();
        fs::create_dir_all(&workspace).expect("workspace should create");

        let default_dir = sessions_dir(&workspace, "default").expect("default dir");
        let profile_dir = sessions_dir(&workspace, "coolzhu-dev").expect("profile dir");

        assert!(default_dir.ends_with(".claw\\sessions"));
        assert!(profile_dir.ends_with(".claw\\sessions\\coolzhu-dev"));

        fs::remove_dir_all(&workspace).expect("workspace should clean");
    }

    #[test]
    fn prune_keeps_only_latest_ten_sessions() {
        let workspace = temp_workspace();
        let profile = "default";
        fs::create_dir_all(&workspace).expect("workspace should create");

        for index in 0..(MAX_MANAGED_SESSIONS + 2) {
            let handle =
                create_managed_session_handle(&workspace, profile).expect("handle should create");
            let mut session = runtime::Session::new();
            session
                .messages
                .push(ConversationMessage::user_text(format!("message-{index}")));
            save_session(&handle, &session).expect("session should save");
            thread::sleep(Duration::from_millis(2));
        }

        prune_managed_sessions(&workspace, profile, None).expect("prune should succeed");
        let listed = list_managed_sessions(&workspace, profile).expect("sessions should list");
        assert_eq!(listed.len(), MAX_MANAGED_SESSIONS);

        fs::remove_dir_all(&workspace).expect("workspace should clean");
    }

    #[test]
    fn rename_updates_handle_and_file_name() {
        let workspace = temp_workspace();
        let profile = "default";
        fs::create_dir_all(&workspace).expect("workspace should create");

        let handle =
            create_managed_session_handle(&workspace, profile).expect("handle should create");
        let mut session = runtime::Session::new();
        session
            .messages
            .push(ConversationMessage::user_text("hello session"));
        save_session(&handle, &session).expect("session should save");

        let renamed =
            rename_session(&handle, "coolzhu test session").expect("session should rename");
        assert_eq!(renamed.id, "coolzhu test session");
        assert!(renamed.path.exists());
        assert!(!handle.path.exists());

        let listed = list_managed_sessions(&workspace, profile).expect("sessions should list");
        assert_eq!(listed[0].id, "coolzhu test session");

        fs::remove_dir_all(&workspace).expect("workspace should clean");
    }
}
