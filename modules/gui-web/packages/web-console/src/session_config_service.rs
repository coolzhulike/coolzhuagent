//! Web会话配置服务：组织快照、受保护恢复日志、参数发布和会话更新；不格式化HTTP响应。
//! 复用既有存储与配置发布，不拥有第二份状态，不持std锁跨await。
use super::{
    agent_session_backend, api_error, local_chat_runtime_config_from_config,
    mutate_workspace_config, normalize_model, normalize_model_type, normalize_provider,
    normalize_session_avatar, read_config, session_model_limit_response, validate_reasoning_effort,
    validate_session_model_settings, AgentSessionDto, ApiResult, LocalChatRuntimeConfig,
    SessionModelLimitOverride, SessionModelLimitResponse, SessionModelLimitUpdateRequest,
    SessionModelSettingsUpdateRequest, SessionSummaryDto, TrackedSessionStore,
};
use axum::http::{HeaderMap, StatusCode};

pub(super) fn configuration_request_pin(headers: &HeaderMap) -> ApiResult<super::workspace_activity::WorkspacePin> {
    let pin = super::workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let workspace_id = super::workspace_identity(&super::active_workspace_path());
    let scope = pin.configuration_scope(&workspace_id);
    for (name, actual) in [("x-coolzhu-workspace-id", workspace_id.as_str()), ("x-coolzhu-configuration-scope", scope.as_str())] {
        if let Some(expected) = headers.get(name) {
            let expected = expected.to_str().map_err(|_| api_error(StatusCode::BAD_REQUEST, "配置工程标识格式无效"))?;
            if expected != actual {
                return Err(api_error(StatusCode::CONFLICT, "配置页所属工程已切换、重载或重启；草稿未提交，请刷新页面后重新打开配置页"));
            }
        }
    }
    Ok(pin)
}

pub(super) struct Snapshot {
    pub(super) session: SessionSummaryDto,
    pub(super) agent: AgentSessionDto,
    pub(super) parameters: SessionModelLimitOverride,
    pub(super) configuration_revision: u64,
    pub(super) local: LocalChatRuntimeConfig,
}

pub(super) struct SessionConfigService<'a> {
    sessions: &'a TrackedSessionStore,
    // 复用既有工程活动 pin；不持 MutexGuard，覆盖参数操作及响应派生。
    _workspace_pin: super::workspace_activity::WorkspacePin,
}

impl<'a> SessionConfigService<'a> {
    pub(super) fn new(sessions: &'a TrackedSessionStore, headers: &HeaderMap) -> ApiResult<Self> {
        let pin = configuration_request_pin(headers)?;
        Ok(Self { sessions, _workspace_pin: pin })
    }
    pub(super) fn configuration_scope(&self) -> String {
        self._workspace_pin.configuration_scope(&super::workspace_identity(&super::active_workspace_path()))
    }
    pub(super) fn read(&self, session_id: String) -> ApiResult<Snapshot> {
        // 与保存共用 store→config 锁顺序；会话、参数、版本和本地容量属于同一读取快照。
        // 捕获后立即释放锁，插件目录读取和响应编码不占用会话存储锁。
        let (session, agent, parameters, configuration_revision, local) = {
            let store = self
                .sessions
                .lock()
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
            let session = store
                .find_session(&session_id)
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "会话不存在"))?;
            let (parameters, revision, local) = read_config(|config| {
                (
                    config
                        .session_model_limits
                        .get(&session_id)
                        .cloned()
                        .unwrap_or_default(),
                    config.configuration_revision,
                    local_chat_runtime_config_from_config(config),
                )
            });
            (
                session
                    .summary_with_model_settings(store.is_active(&session_id), parameters.clone()),
                session.to_agent_session_with_model_settings(parameters.clone()),
                parameters,
                revision,
                local,
            )
        };
        Ok(Snapshot {
            session,
            agent,
            parameters,
            configuration_revision,
            local,
        })
    }
    pub(super) fn read_limit(&self, session_id: String) -> ApiResult<SessionModelLimitResponse> {
        let response = {
            let store = self
                .sessions
                .lock()
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
            let session = store
                .find_session(&session_id)
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "会话不存在"))?;
            if agent_session_backend::AgentSessionBackend::for_provider(&session.provider)
                != agent_session_backend::AgentSessionBackend::LlmHttp
            {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "Devin 容量尚未协商，请使用模型与会话配置页查看状态。",
                ));
            }
            let (parameters, local) = read_config(|config| {
                (
                    config
                        .session_model_limits
                        .get(&session_id)
                        .cloned()
                        .unwrap_or_default(),
                    local_chat_runtime_config_from_config(config),
                )
            });
            session_model_limit_response(
                session.to_agent_session_with_model_settings(parameters.clone()),
                parameters,
                local,
            )
        };
        Ok(response)
    }
    pub(super) fn save_limit(
        &self,
        session_id: String,
        payload: SessionModelLimitUpdateRequest,
    ) -> ApiResult<SessionModelLimitResponse> {
        let response = {
            let store = self
                .sessions
                .lock()
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
            let session = store
                .find_session(&session_id)
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "会话不存在"))?;
            if agent_session_backend::AgentSessionBackend::for_provider(&session.provider)
                != agent_session_backend::AgentSessionBackend::LlmHttp
            {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "Devin 容量尚未协商，请使用模型与会话配置页查看状态。",
                ));
            }
            // 与统一保存沿用 store→config 顺序；响应只使用本次已发布配置。
            let published = mutate_workspace_config(|config| {
                if payload.expected_revision.is_some_and(|revision| revision != config.configuration_revision) {
                    return Err(api_error(StatusCode::CONFLICT, "配置已被其他窗口或任务更新，请重新载入后再保存；当前草稿未写入"));
                }
                // 旧容量 API 只修改容量，不能丢弃采样、协议和工具参数。
                let settings = config
                    .session_model_limits
                    .entry(session_id.clone())
                    .or_default();
                settings.context_window = payload.context_window.min(4_000_000);
                settings.max_output_tokens = payload.max_output_tokens.min(1_000_000);
                Ok(())
            })?;
            let parameters = published
                .session_model_limits
                .get(&session_id)
                .cloned()
                .unwrap_or_default();
            let local = local_chat_runtime_config_from_config(&published);
            session_model_limit_response(
                session.to_agent_session_with_model_settings(parameters.clone()),
                parameters,
                local,
            )
        };
        Ok(response)
    }
    pub(super) fn save(&self,session_id:String,payload:SessionModelSettingsUpdateRequest)->ApiResult<()> {
        self.save_or_create(Some(session_id),payload).map(|_|())
    }
    pub(super) fn create(&self,payload:SessionModelSettingsUpdateRequest)->ApiResult<String> {
        self.save_or_create(None,payload)
    }
    fn save_or_create(&self,existing_id:Option<String>,payload:SessionModelSettingsUpdateRequest)->ApiResult<String> {
        validate_form_fields(payload.session.as_ref(),existing_id.is_none())?;
        let result_id;
        // 与普通会话修改/删除共用存储锁；配置锁只在 mutator 内按 store→config 顺序取得。
        // 同步发布块结束后再 await，避免持有 std MutexGuard 跨异步边界。
        {
            let mut store = self
                .sessions
                .lock()
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话存储锁已损坏"))?;
            let new_session=if existing_id.is_none() {Some(store.prepare_new_session(payload.session.clone()
                .ok_or_else(||api_error(StatusCode::BAD_REQUEST,"新会话缺少模型配置"))?)?)}else{None};
            let session_id=existing_id.or_else(||new_session.as_ref().map(|s|s.id.clone())).unwrap();
            result_id=session_id.clone();
            let mut agent = new_session.as_ref().or_else(||store.find_session(&session_id))
                .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "会话不存在"))?
                .to_agent_session(store.is_active(&session_id));
            if let Some(session) = payload.session.as_ref() {
                validate_reasoning_effort(session.reasoning_effort.as_deref())?;
                if let Some(avatar) = session.avatar.as_deref() {
                    normalize_session_avatar(Some(avatar))?;
                }
                if let Some(provider) = &session.provider {
                    agent.provider = normalize_provider(Some(provider));
                }
                if let Some(model_type) = &session.model_type {
                    agent.model_type = normalize_model_type(Some(model_type));
                }
                agent_session_backend::validate_session_input(session, &agent.provider)?;
                if let Some(model) = &session.model {
                    agent.model = normalize_model(Some(model));
                }
                if let Some(effort) = &session.reasoning_effort {
                    agent.reasoning_effort = effort.clone();
                }
            }
            validate_session_model_settings(&payload.parameters, &agent)?;
            // 两种资源的发布全过程持有既有配置锁，其他配置操作不能插入。
            let mut guard = super::workspace_config().lock()
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "配置锁已损坏"))?;
            if payload.expected_revision.is_some_and(|revision| revision != guard.configuration_revision) {
                return Err(api_error(StatusCode::CONFLICT, "配置已被其他窗口或任务更新，请重新载入后再保存；当前草稿未写入"));
            }
            let workspace = super::active_workspace_path();
            let path = super::config_file_path_for(&workspace);
            let before = std::fs::read_to_string(&path).map_err(super::io_api_error)?;
            // 不把不可解析的现有配置覆盖成默认内容。
            toml::from_str::<toml::Value>(&before)
                .map_err(|_| api_error(StatusCode::CONFLICT, "已有配置格式无效，未发布"))?;
            let mut published = guard.clone();
            published.configuration_revision = guard.configuration_revision.checked_add(1)
                .ok_or_else(|| api_error(StatusCode::CONFLICT, "配置版本已耗尽，未保存"))?;
            published.session_model_limits.insert(session_id.clone(), payload.parameters);
            let serialized = toml::to_string_pretty(&published)
                .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "配置无法序列化"))?;
            let after = super::render_config_with_preserved_sections(&path, &serialized);
            let operation = super::session_config_journal::begin(&store.path, &path, &before, &after)
                .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
            if let Err(message) = super::config_publication::publish(&path, &after) {
                super::session_config_journal::recover(&store.path, &path)
                    .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
                return Err(api_error(StatusCode::INTERNAL_SERVER_ERROR, &message));
            }
            let result = if let Some(session)=new_session {
                store.state.active_session_id=Some(session.id.clone());store.state.sessions.push(session);
                store.save_with_config_operation(Some(&operation))
            } else {match payload.session {
                Some(session) => store.update_session_with_config_operation(&session_id, session, Some(&operation)),
                None => store.save_with_config_operation(Some(&operation)),
            }};
            if let Err(error) = result {
                super::session_config_journal::verify_rollback_allowed(&store.path)
                    .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
                super::session_config_journal::recover(&store.path, &path)
                    .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
                return Err(error);
            }
            *guard = published;
            super::session_config_journal::recover(&store.path, &path)
                .map_err(|_| api_error(StatusCode::CONFLICT, "配置与会话已提交，但恢复日志清理未完成；请重新载入核对状态"))?;
        }
        Ok(result_id)
    }
}

fn validate_form_fields(session:Option<&super::UpsertSessionRequest>,creating:bool)->ApiResult<()> {
    for (label,value) in [("会话名称",session.and_then(|s|s.name.as_deref())),("模型 ID",session.and_then(|s|s.model.as_deref()))] {
        if value.is_some_and(|v|v.trim().is_empty()) || (creating && value.is_none()) {
            return Err(api_error(StatusCode::BAD_REQUEST,&format!("{label}不能为空")));
        }
    }
    Ok(())
}

#[cfg(test)] mod form_tests {
    use super::*;
    #[test] fn 新建必须填写名称和模型且参数单独修改仍然可用() {
        let session=|value|serde_json::from_value::<super::super::UpsertSessionRequest>(value).unwrap();
        assert!(validate_form_fields(None,true).is_err());
        assert!(validate_form_fields(Some(&session(serde_json::json!({"name":"test","model":" "}))),true).is_err());
        assert!(validate_form_fields(Some(&session(serde_json::json!({"name":"test","model":"model"}))),true).is_ok());
        assert!(validate_form_fields(None,false).is_ok());
        assert!(validate_form_fields(Some(&session(serde_json::json!({"name":" "}))),false).is_err());
    }
}
