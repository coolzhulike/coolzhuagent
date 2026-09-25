use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use eframe::egui::{self, Align, Color32, Layout, RichText, ScrollArea, TextEdit, Vec2};
use runtime::{ContentBlock, ConversationMessage, Session};

use crate::config::{
    desktop_demo_config_path, load_gui_config_from_path, write_gui_config_to_path, GuiConfig,
    GuiConfigStore,
};
use crate::desktop_agent::{
    mode_label, phase_label, spawn_desktop_automation, DesktopAutomationEvent,
    DesktopAutomationPhase, DesktopAutomationSnapshot,
};
use crate::desktop_capture::DesktopCaptureState;
use crate::service::{
    classify_prompt_route, session_to_gui_messages, AgentRoute, GuiAgentService, GuiMessage,
    GuiReply, GuiRole,
};
use crate::sessions::{
    create_managed_session_handle, delete_session, list_managed_sessions, load_session,
    prune_managed_sessions, rename_session, save_session, GuiManagedSessionSummary,
    GuiSessionHandle,
};
use crate::theme;

const TRACE_LIMIT: usize = 40;
const INITIAL_VISIBLE_MESSAGES: usize = 30;
const MESSAGE_LOAD_STEP: usize = 30;

pub struct ClawGuiApp {
    config_store: GuiConfigStore,
    config_draft: GuiConfig,
    service: GuiAgentService,
    desktop_capture: DesktopCaptureState,
    conversation_session: Session,
    messages: Vec<GuiMessage>,
    current_session: Option<GuiSessionHandle>,
    managed_sessions: Vec<GuiManagedSessionSummary>,
    visible_message_count: usize,
    renaming_session_id: Option<String>,
    rename_draft: String,
    traces: VecDeque<RuntimeTrace>,
    composer: String,
    live_status: String,
    request_status: String,
    config_status: String,
    active_route: Option<AgentRoute>,
    last_route: AgentRoute,
    inflight: bool,
    pending_rx: Option<Receiver<Result<GuiReply, String>>>,
    desktop_automation_rx: Option<Receiver<DesktopAutomationEvent>>,
    desktop_automation: Option<DesktopAutomationSnapshot>,
    pending_desktop_automation_prompt: Option<String>,
    desktop_automation_launch_after: Option<Instant>,
    viewport_hidden_for_automation: bool,
    viewport_hidden_applied: bool,
    show_config_panel: bool,
    trace_sequence: u64,
    config_import_path: String,
    demo_config_path: PathBuf,
    test_resolution: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct RuntimeTrace {
    sequence: u64,
    phase: String,
    detail: String,
}

impl ClawGuiApp {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let (config_store, config_status) = match GuiConfigStore::load_or_create(&cwd) {
            Ok(store) => (
                store,
                "\u{754c}\u{9762}\u{914d}\u{7f6e}\u{5df2}\u{81ea}\u{52a8}\u{52a0}\u{8f7d}\u{3002}"
                    .to_string(),
            ),
            Err(error) => (
                GuiConfigStore::fallback(&cwd),
                format!(
                    "\u{754c}\u{9762}\u{914d}\u{7f6e}\u{52a0}\u{8f7d}\u{5931}\u{8d25},\u{5df2}\u{56de}\u{9000}\u{5230}\u{9ed8}\u{8ba4}\u{503c}:{error}"
                ),
            ),
        };

        let config_draft = config_store.config().clone();
        let service = GuiAgentService::from_config(config_draft.clone());
        let live_status = if service.live_mode_enabled() {
            "\u{5728}\u{7ebf}".to_string()
        } else {
            "\u{6f14}\u{793a}".to_string()
        };
        let demo_config_path = desktop_demo_config_path(&cwd);

        let mut app = Self {
            config_store,
            config_draft,
            service,
            desktop_capture: DesktopCaptureState::new(),
            conversation_session: Session::new(),
            messages: Vec::new(),
            current_session: None,
            managed_sessions: Vec::new(),
            visible_message_count: INITIAL_VISIBLE_MESSAGES,
            renaming_session_id: None,
            rename_draft: String::new(),
            traces: VecDeque::new(),
            composer: String::new(),
            live_status,
            request_status: "\u{7a7a}\u{95f2}".to_string(),
            config_status,
            active_route: None,
            last_route: AgentRoute::Chat,
            inflight: false,
            pending_rx: None,
            desktop_automation_rx: None,
            desktop_automation: None,
            pending_desktop_automation_prompt: None,
            desktop_automation_launch_after: None,
            viewport_hidden_for_automation: false,
            viewport_hidden_applied: false,
            show_config_panel: false,
            trace_sequence: 0,
            config_import_path: String::new(),
            demo_config_path,
            test_resolution: "1920x1080".to_string(),
        };

        app.config_import_path = app.demo_config_path.display().to_string();
        app.push_trace(
            "\u{914d}\u{7f6e}\u{52a0}\u{8f7d}",
            app.config_status.clone(),
        );

        let demo_note =
            match write_gui_config_to_path(&app.demo_config_path, app.config_store.config()) {
                Ok(()) => format!(
                    "\u{5df2}\u{5237}\u{65b0}\u{793a}\u{4f8b}\u{914d}\u{7f6e}:{}",
                    app.demo_config_path.display()
                ),
                Err(error) => format!(
                    "\u{793a}\u{4f8b}\u{914d}\u{7f6e}\u{5237}\u{65b0}\u{5931}\u{8d25}:{error}"
                ),
            };
        app.push_trace("\u{793a}\u{4f8b}\u{914d}\u{7f6e}", demo_note);
        app.refresh_session_list();

        app
    }

    fn push_trace(&mut self, phase: impl Into<String>, detail: impl Into<String>) {
        self.trace_sequence += 1;
        self.traces.push_front(RuntimeTrace {
            sequence: self.trace_sequence,
            phase: phase.into(),
            detail: detail.into(),
        });
        while self.traces.len() > TRACE_LIMIT {
            self.traces.pop_back();
        }
    }

    fn refresh_service(&mut self, note: impl Into<String>) {
        self.service = GuiAgentService::from_config(self.config_store.config().clone());
        self.live_status = if self.service.live_mode_enabled() {
            "\u{5728}\u{7ebf}".to_string()
        } else {
            "\u{6f14}\u{793a}".to_string()
        };
        self.config_status = note.into();
        self.push_trace(
            "\u{670d}\u{52a1}\u{91cd}\u{5efa}",
            self.config_status.clone(),
        );
        let _ = write_gui_config_to_path(&self.demo_config_path, self.config_store.config());
        self.refresh_session_list();
    }

    fn active_session_profile(&self) -> &str {
        self.service.profile()
    }

    fn refresh_session_list(&mut self) {
        if let Err(error) = prune_managed_sessions(
            self.service.workspace(),
            self.active_session_profile(),
            self.current_session
                .as_ref()
                .map(|handle| handle.id.as_str()),
        ) {
            self.push_trace(
                "\u{4f1a}\u{8bdd}",
                format!("\u{4f1a}\u{8bdd}\u{6574}\u{7406}\u{5931}\u{8d25}:{error}"),
            );
        }

        match list_managed_sessions(self.service.workspace(), self.active_session_profile()) {
            Ok(sessions) => {
                self.managed_sessions = sessions;
            }
            Err(error) => {
                self.managed_sessions.clear();
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!(
                        "\u{8bfb}\u{53d6}\u{4f1a}\u{8bdd}\u{5217}\u{8868}\u{5931}\u{8d25}:{error}"
                    ),
                );
            }
        }
    }

    fn ensure_current_session(&mut self) -> Result<(), String> {
        if self.current_session.is_none() {
            self.current_session = Some(create_managed_session_handle(
                self.service.workspace(),
                self.active_session_profile(),
            )?);
        }
        Ok(())
    }

    fn persist_current_session(&mut self) {
        if self.messages.is_empty() && self.current_session.is_none() {
            return;
        }

        if let Err(error) = self.ensure_current_session() {
            self.push_trace(
                "\u{4f1a}\u{8bdd}",
                format!("\u{521b}\u{5efa}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
            );
            return;
        }

        if let Some(handle) = self.current_session.clone() {
            if let Err(error) = save_session(&handle, &self.conversation_session) {
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!("\u{4fdd}\u{5b58}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                );
                return;
            }
            self.refresh_session_list();
        }
    }

    fn create_new_session(&mut self) {
        match create_managed_session_handle(self.service.workspace(), self.active_session_profile())
        {
            Ok(handle) => {
                self.current_session = Some(handle.clone());
                self.conversation_session = Session::new();
                self.messages.clear();
                self.visible_message_count = INITIAL_VISIBLE_MESSAGES;
                self.request_status = "\u{65b0}\u{4f1a}\u{8bdd}".to_string();
                if let Err(error) = save_session(&handle, &self.conversation_session) {
                    self.push_trace(
                        "\u{4f1a}\u{8bdd}",
                        format!("\u{521b}\u{5efa}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                    );
                } else {
                    self.refresh_session_list();
                }
            }
            Err(error) => {
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!("\u{521b}\u{5efa}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                );
            }
        }
    }

    fn open_session(&mut self, handle: GuiSessionHandle) {
        self.persist_current_session();
        match load_session(&handle) {
            Ok(session) => {
                self.conversation_session = session.clone();
                self.messages = session_to_gui_messages(&session);
                self.visible_message_count =
                    self.messages.len().min(INITIAL_VISIBLE_MESSAGES).max(1);
                self.current_session = Some(handle);
                self.request_status = "\u{5df2}\u{6253}\u{5f00}\u{4f1a}\u{8bdd}".to_string();
                self.refresh_session_list();
            }
            Err(error) => {
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!("\u{6253}\u{5f00}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                );
            }
        }
    }

    fn start_rename_session(&mut self, handle: &GuiSessionHandle) {
        self.renaming_session_id = Some(handle.id.clone());
        self.rename_draft = handle.id.clone();
    }

    fn commit_rename_session(&mut self, handle: GuiSessionHandle) {
        match rename_session(&handle, &self.rename_draft) {
            Ok(renamed) => {
                if self.current_session.as_ref() == Some(&handle) {
                    self.current_session = Some(renamed.clone());
                }
                self.renaming_session_id = None;
                self.rename_draft.clear();
                self.request_status =
                    "\u{4f1a}\u{8bdd}\u{5df2}\u{91cd}\u{547d}\u{540d}".to_string();
                self.refresh_session_list();
            }
            Err(error) => {
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!("\u{91cd}\u{547d}\u{540d}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                );
            }
        }
    }

    fn delete_session(&mut self, handle: GuiSessionHandle) {
        match delete_session(&handle) {
            Ok(()) => {
                if self.current_session.as_ref() == Some(&handle) {
                    self.current_session = None;
                    self.conversation_session = Session::new();
                    self.messages.clear();
                    self.visible_message_count = INITIAL_VISIBLE_MESSAGES;
                    self.request_status = "\u{4f1a}\u{8bdd}\u{5df2}\u{5220}\u{9664}".to_string();
                }
                if self.renaming_session_id.as_deref() == Some(handle.id.as_str()) {
                    self.renaming_session_id = None;
                    self.rename_draft.clear();
                }
                self.refresh_session_list();
            }
            Err(error) => {
                self.push_trace(
                    "\u{4f1a}\u{8bdd}",
                    format!("\u{5220}\u{9664}\u{4f1a}\u{8bdd}\u{5931}\u{8d25}:{error}"),
                );
            }
        }
    }

    fn poll_background_reply(&mut self) {
        let Some(receiver) = &self.pending_rx else {
            return;
        };
        let Ok(result) = receiver.try_recv() else {
            return;
        };

        self.inflight = false;
        self.pending_rx = None;
        let completed_route = self.active_route.unwrap_or(self.last_route);
        self.active_route = None;

        match result {
            Ok(reply) => {
                if completed_route == AgentRoute::DesktopVision {
                    self.desktop_capture.force_refresh();
                }
                self.request_status = format!(
                    "\u{5b8c}\u{6210} | \u{6a21}\u{578b}={} | \u{4ee4}\u{724c}={}{}",
                    reply.model,
                    reply.total_tokens,
                    reply
                        .request_id
                        .as_deref()
                        .map_or_else(String::new, |id| format!(" | request={id}")),
                );
                self.push_trace(
                    "\u{6a21}\u{578b}\u{54cd}\u{5e94}",
                    format!(
                        "\u{56de}\u{590d}\u{5df2}\u{8fd4}\u{56de},\u{6a21}\u{578b}={},\u{4ee4}\u{724c}={}.",
                        reply.model, reply.total_tokens
                    ),
                );
                self.push_trace(
                    "\u{4ee3}\u{7406}\u{72b6}\u{6001}",
                    format!(
                        "{} \u{5df2}\u{5b8c}\u{6210}\u{5f53}\u{524d}\u{4efb}\u{52a1}\u{3002}",
                        route_name(completed_route)
                    ),
                );
                self.messages.push(GuiMessage {
                    role: GuiRole::Assistant,
                    text: reply.text.clone(),
                });
                self.visible_message_count =
                    (self.visible_message_count + 1).min(self.messages.len().max(1));
                self.conversation_session
                    .messages
                    .push(ConversationMessage::assistant(vec![ContentBlock::Text {
                        text: reply.text,
                    }]));
                self.persist_current_session();
            }
            Err(error) => {
                self.request_status = "\u{5931}\u{8d25}".to_string();
                self.push_trace(
                    "\u{6a21}\u{578b}\u{54cd}\u{5e94}",
                    format!("\u{8bf7}\u{6c42}\u{5931}\u{8d25}:{error}"),
                );
                self.push_trace(
                    "\u{4ee3}\u{7406}\u{72b6}\u{6001}",
                    format!(
                        "{} \u{6267}\u{884c}\u{5931}\u{8d25}\u{3002}",
                        route_name(completed_route)
                    ),
                );
                self.messages.push(GuiMessage {
                    role: GuiRole::Assistant,
                    text: format!("\u{8bf7}\u{6c42}\u{5931}\u{8d25}:{error}"),
                });
            }
        }
    }

    fn start_desktop_automation(&mut self, prompt: String) {
        if !self.service.live_mode_enabled() {
            self.inflight = false;
            self.active_route = None;
            self.request_status =
                "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{9700}\u{8981}\u{53ef}\u{7528}\u{7684} API Key"
                    .to_string();
            self.messages.push(GuiMessage {
                role: GuiRole::Assistant,
                text: "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{9700}\u{8981}\u{53ef}\u{7528}\u{7684}\u{5728}\u{7ebf}\u{89c6}\u{89c9}\u{6a21}\u{578b} API Key\u{3002}"
                    .to_string(),
            });
            return;
        }

        self.request_status =
            "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{6b63}\u{5728}\u{63a5}\u{7ba1}\u{4efb}\u{52a1}\u{2026}"
                .to_string();
        self.push_trace(
            "\u{684c}\u{9762}\u{4ee3}\u{7406}",
            "\u{5df2}\u{542f}\u{52a8}\u{540e}\u{53f0}\u{684c}\u{9762}\u{4efb}\u{52a1}\u{76d1}\u{7763}\u{5668}\u{3002}"
                .to_string(),
        );
        self.desktop_automation = None;
        self.desktop_automation_rx = None;
        self.pending_desktop_automation_prompt = Some(prompt);
        self.desktop_automation_launch_after = None;
        self.viewport_hidden_for_automation = true;
    }

    fn poll_desktop_automation(&mut self) {
        let Some(receiver) = &self.desktop_automation_rx else {
            return;
        };
        let Ok(event) = receiver.try_recv() else {
            return;
        };

        match event {
            DesktopAutomationEvent::Progress(snapshot) => {
                self.request_status = format!(
                    "{} | {}/{} | {}",
                    phase_label(snapshot.phase),
                    snapshot.step,
                    snapshot.max_steps,
                    snapshot.detail
                );
                self.push_trace(
                    "\u{684c}\u{9762}\u{4ee3}\u{7406}",
                    format!("{} | {}", phase_label(snapshot.phase), snapshot.detail),
                );
                self.desktop_automation = Some(snapshot);
            }
            DesktopAutomationEvent::Finished(result) => {
                self.inflight = false;
                self.desktop_automation_rx = None;
                self.pending_desktop_automation_prompt = None;
                self.desktop_automation_launch_after = None;
                self.active_route = None;
                self.last_route = AgentRoute::DesktopAutomation;
                self.viewport_hidden_for_automation = false;
                self.desktop_capture.force_refresh();

                match result {
                    Ok(result) => {
                        self.request_status = format!(
                            "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{5b8c}\u{6210} | {} | {}",
                            mode_label(result.mode),
                            result.summary
                        );
                        self.push_trace(
                            "\u{684c}\u{9762}\u{4ee3}\u{7406}",
                            format!(
                                "\u{4efb}\u{52a1}\u{5b8c}\u{6210},\u{5171} {} \u{6b65},\u{6a21}\u{5f0f}: {},\u{6a21}\u{578b}: {},\u{76ee}\u{6807}: {}。",
                                result.steps_completed,
                                mode_label(result.mode),
                                result.model,
                                result.target_window.as_deref().unwrap_or("\u{672a}\u{77e5}")
                            ),
                        );
                        self.messages.push(GuiMessage {
                            role: GuiRole::Assistant,
                            text: result.summary.clone(),
                        });
                        self.visible_message_count =
                            (self.visible_message_count + 1).min(self.messages.len().max(1));
                        self.conversation_session
                            .messages
                            .push(ConversationMessage::assistant(vec![ContentBlock::Text {
                                text: result.summary,
                            }]));
                        self.persist_current_session();
                    }
                    Err(error) => {
                        self.request_status =
                            "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{5931}\u{8d25}".to_string();
                        self.push_trace(
                            "\u{684c}\u{9762}\u{4ee3}\u{7406}",
                            format!("\u{4efb}\u{52a1}\u{5931}\u{8d25}:{error}"),
                        );
                        if let Some(snapshot) = self.desktop_automation.as_mut() {
                            snapshot.phase = DesktopAutomationPhase::Failed;
                            snapshot.detail = error.clone();
                        }
                        self.messages.push(GuiMessage {
                            role: GuiRole::Assistant,
                            text: format!(
                                "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{5931}\u{8d25}:{error}"
                            ),
                        });
                        self.visible_message_count =
                            (self.visible_message_count + 1).min(self.messages.len().max(1));
                        self.conversation_session
                            .messages
                            .push(ConversationMessage::assistant(vec![ContentBlock::Text {
                                text: format!(
                                    "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{5931}\u{8d25}:{error}"
                                ),
                            }]));
                        self.persist_current_session();
                    }
                }
            }
        }
    }

    fn sync_automation_viewport(&mut self, ctx: &egui::Context) {
        if self.viewport_hidden_for_automation && !self.viewport_hidden_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            self.viewport_hidden_applied = true;
            self.desktop_automation_launch_after =
                Some(Instant::now() + Duration::from_millis(280));
        } else if self.viewport_hidden_for_automation && self.viewport_hidden_applied {
            if self.desktop_automation_rx.is_none()
                && self
                    .desktop_automation_launch_after
                    .is_some_and(|deadline| Instant::now() >= deadline)
            {
                if let Some(prompt) = self.pending_desktop_automation_prompt.take() {
                    self.desktop_automation_rx = Some(spawn_desktop_automation(
                        self.config_store.config().clone(),
                        prompt,
                    ));
                }
            }
        } else if !self.viewport_hidden_for_automation && self.viewport_hidden_applied {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
            self.viewport_hidden_applied = false;
            self.desktop_automation_launch_after = None;
        }
    }

    fn submit(&mut self) {
        if self.inflight || self.composer.trim().is_empty() {
            return;
        }

        let prompt = self.composer.trim().to_string();
        let route = classify_prompt_route(&prompt);
        self.messages.push(GuiMessage {
            role: GuiRole::User,
            text: prompt.clone(),
        });
        self.visible_message_count = (self.visible_message_count + 1).min(self.messages.len());
        self.conversation_session
            .messages
            .push(ConversationMessage::user_text(prompt.clone()));
        self.composer.clear();
        self.inflight = true;
        self.active_route = Some(route);
        self.last_route = route;
        self.persist_current_session();
        self.request_status = if self.service.live_mode_enabled() {
            match route {
                AgentRoute::Chat => {
                    "\u{4e3b}\u{4ee3}\u{7406}\u{6b63}\u{5728}\u{8bf7}\u{6c42}\u{667a}\u{8c31}\u{6a21}\u{578b}\u{2026}"
                        .to_string()
                }
                AgentRoute::DesktopVision => "\u{89c6}\u{89c9}\u{4ee3}\u{7406}\u{6b63}\u{5728}\u{5206}\u{6790}\u{6700}\u{65b0}\u{684c}\u{9762}\u{622a}\u{56fe}\u{2026}"
                    .to_string(),
                AgentRoute::DesktopAutomation => "\u{684c}\u{9762}\u{4ee3}\u{7406}\u{6b63}\u{5728}\u{51c6}\u{5907}\u{540e}\u{53f0}\u{64cd}\u{4f5c}\u{2026}"
                    .to_string(),
            }
        } else {
            "\u{5f53}\u{524d}\u{5904}\u{4e8e}\u{6f14}\u{793a}\u{6a21}\u{5f0f},\u{6b63}\u{5728}\u{751f}\u{6210}\u{672c}\u{5730}\u{56de}\u{5e94}\u{2026}"
                .to_string()
        };

        self.push_trace(
            "\u{7528}\u{6237}\u{8f93}\u{5165}",
            format!("\u{5df2}\u{63d0}\u{4ea4}\u{65b0}\u{6d88}\u{606f}:{prompt}"),
        );
        self.push_trace(
            "\u{4ee3}\u{7406}\u{8def}\u{7531}",
            format!(
                "\u{672c}\u{6b21}\u{8bf7}\u{6c42}\u{5df2}\u{5206}\u{914d}\u{7ed9}{}。",
                route_name(route)
            ),
        );

        if route == AgentRoute::DesktopAutomation {
            self.start_desktop_automation(prompt);
            return;
        }

        let transcript = self.conversation_session.clone();
        let service = self.service.clone();
        let (tx, rx) = mpsc::channel();
        self.pending_rx = Some(rx);
        thread::spawn(move || {
            let result = service.send_chat(&transcript, &prompt);
            let _ = tx.send(result);
        });
    }

    fn save_and_apply_config(&mut self) {
        self.config_store.replace(self.config_draft.clone());
        match self.config_store.save() {
            Ok(()) => {
                self.refresh_service(format!(
                    "\u{914d}\u{7f6e}\u{5df2}\u{4fdd}\u{5b58}\u{5e76}\u{5e94}\u{7528}:{}",
                    self.config_store.path().display()
                ));
                self.push_trace(
                    "\u{914d}\u{7f6e}\u{66f4}\u{65b0}",
                    "\u{540e}\u{7eed}\u{8bf7}\u{6c42}\u{4f1a}\u{4f7f}\u{7528}\u{6700}\u{65b0}\u{914d}\u{7f6e}\u{3002}"
                        .to_string(),
                );
            }
            Err(error) => {
                self.config_status = format!("\u{4fdd}\u{5b58}\u{5931}\u{8d25}:{error}");
                self.push_trace(
                    "\u{914d}\u{7f6e}\u{5199}\u{5165}",
                    self.config_status.clone(),
                );
            }
        }
    }

    fn reload_config(&mut self) {
        match self.config_store.reload_from_disk() {
            Ok(()) => {
                self.config_draft = self.config_store.config().clone();
                self.refresh_service(format!(
                    "\u{5df2}\u{4ece}\u{78c1}\u{76d8}\u{91cd}\u{65b0}\u{52a0}\u{8f7d}\u{914d}\u{7f6e}:{}",
                    self.config_store.path().display()
                ));
            }
            Err(error) => {
                self.config_status = format!("\u{91cd}\u{8f7d}\u{5931}\u{8d25}:{error}");
                self.push_trace(
                    "\u{914d}\u{7f6e}\u{91cd}\u{8f7d}",
                    self.config_status.clone(),
                );
            }
        }
    }

    fn import_config_file(&mut self) {
        let path = PathBuf::from(self.config_import_path.trim());
        match load_gui_config_from_path(&path) {
            Ok(imported) => {
                self.config_draft = imported.clone();
                self.config_store.replace(imported);
                match self.config_store.save() {
                    Ok(()) => {
                        self.refresh_service(format!(
                            "\u{5df2}\u{5bfc}\u{5165} JSON \u{914d}\u{7f6e}\u{5e76}\u{5199}\u{5165}:{}",
                            self.config_store.path().display()
                        ));
                        self.push_trace(
                            "JSON \u{5bfc}\u{5165}",
                            format!("\u{5bfc}\u{5165}\u{6765}\u{6e90}:{}", path.display()),
                        );
                    }
                    Err(error) => {
                        self.config_status = format!(
                            "\u{5bfc}\u{5165}\u{540e}\u{4fdd}\u{5b58}\u{5931}\u{8d25}:{error}"
                        );
                        self.push_trace("JSON \u{5bfc}\u{5165}", self.config_status.clone());
                    }
                }
            }
            Err(error) => {
                self.config_status = format!("JSON \u{5bfc}\u{5165}\u{5931}\u{8d25}:{error}");
                self.push_trace("JSON \u{5bfc}\u{5165}", self.config_status.clone());
            }
        }
    }

    fn render_left_hud(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical()
            .id_salt("left-sidebar-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                hud_card(
                    ui,
                    "CZ",
                    "总览",
                    Some(("运行中", theme::SUCCESS)),
                    |ui| {
                        stat_line(ui, "状态", &self.live_status, theme::SUCCESS);
                        stat_line(ui, "模型", self.service.model(), theme::QUESTION);
                        stat_line(ui, "视觉", self.service.vision_model(), theme::QUESTION);
                        stat_line(ui, "连接", self.service.vision_backend(), theme::SUCCESS);
                        stat_line(ui, "日志", "开启", theme::SUCCESS);
                    },
                );

                ui.add_space(10.0);

                hud_card(ui, "DIR", "工程目录", None, |ui| {
                    ui.label(theme::label(&self.config_draft.workspace));
                    stat_line(ui, "档案", self.service.profile(), theme::QUESTION);
                    stat_line(ui, "上下文", self.service.context_engine(), theme::QUESTION);
                });

                ui.add_space(10.0);

                hud_card(ui, "CFG", "配置", None, |ui| {
                    stat_line(ui, "Provider", &self.config_draft.provider, theme::QUESTION);
                    stat_line(ui, "温度", "0.20", theme::QUESTION);
                    let button_label = if self.show_config_panel {
                        "关闭配置"
                    } else {
                        "打开配置"
                    };
                    if ui
                        .add(pixel_button(
                            button_label,
                            theme::QUESTION,
                            theme::PANEL_ALT,
                        ))
                        .clicked()
                    {
                        self.show_config_panel = !self.show_config_panel;
                    }
                });

                ui.add_space(10.0);
                self.render_session_panel(ui);
                ui.add_space(10.0);
                self.render_agent_panel(ui);
                ui.add_space(10.0);

                hud_card(ui, "LOG", "诊断日志", None, |ui| {
                    stat_line(ui, "级别", "INFO", theme::QUESTION);
                    let log_path = diagnostics::log_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "未初始化".to_string());
                    ui.label(theme::label(&log_path));
                });

                ui.add_space(10.0);

                hud_card(ui, "EXT", "扩展", Some(("可用", theme::SUCCESS)), |ui| {
                    stat_line(ui, "插件", "4/4", theme::SUCCESS);
                    stat_line(ui, "Skill", "8/8", theme::SUCCESS);
                    stat_line(ui, "CLI", "在线", theme::SUCCESS);
                });
            });
    }

    fn render_session_panel(&mut self, ui: &mut egui::Ui) {
        hud_card(ui, "SES", "会话", None, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("\u{65b0}\u{5efa}")
                                .monospace()
                                .strong()
                                .color(theme::PANEL_ALT),
                        )
                        .fill(theme::QUESTION),
                    )
                    .clicked()
                {
                    self.create_new_session();
                }

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("\u{5237}\u{65b0}")
                                .monospace()
                                .strong()
                                .color(theme::TEXT_PRIMARY),
                        )
                        .fill(theme::BRICK),
                    )
                    .clicked()
                {
                    self.refresh_session_list();
                }
            });

            ui.add_space(8.0);
            if let Some(handle) = &self.current_session {
                ui.label(theme::label(&format!(
                    "\u{5f53}\u{524d}\u{4f1a}\u{8bdd}:{}",
                    handle.id
                )));
            }

            if self.managed_sessions.is_empty() {
                ui.label(theme::label("\u{6682}\u{65e0}"));
                return;
            }

            let mut open_target = None;
            let mut delete_target = None;
            let mut rename_target = None;
            let mut rename_commit_target = None;
            let mut cancel_rename = false;

            for session in self.managed_sessions.clone() {
                let is_current = self
                    .current_session
                    .as_ref()
                    .is_some_and(|handle| handle.id == session.id);
                let is_renaming = self
                    .renaming_session_id
                    .as_deref()
                    .is_some_and(|id| id == session.id);
                theme::pixel_frame(theme::PANEL).show(ui, |ui| {
                    if is_renaming {
                        let input = ui.add(
                            TextEdit::singleline(&mut self.rename_draft)
                                .desired_width(f32::INFINITY)
                                .hint_text("\u{4f1a}\u{8bdd}\u{540d}\u{79f0}"),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{4fdd}\u{5b58}")
                                            .monospace()
                                            .strong()
                                            .color(theme::PANEL_ALT),
                                    )
                                    .fill(theme::QUESTION),
                                )
                                .clicked()
                            {
                                rename_commit_target = Some(GuiSessionHandle {
                                    id: session.id.clone(),
                                    path: session.path.clone(),
                                });
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{53d6}\u{6d88}")
                                            .monospace()
                                            .strong()
                                            .color(theme::TEXT_PRIMARY),
                                    )
                                    .fill(theme::BRICK),
                                )
                                .clicked()
                            {
                                cancel_rename = true;
                            }
                        });

                        if input.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter))
                        {
                            rename_commit_target = Some(GuiSessionHandle {
                                id: session.id.clone(),
                                path: session.path.clone(),
                            });
                        }
                    } else {
                        ui.label(
                            RichText::new(&session.id)
                                .monospace()
                                .strong()
                                .size(15.0)
                                .color(if is_current {
                                    theme::SUCCESS
                                } else {
                                    theme::QUESTION
                                }),
                        );
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{7ee7}\u{7eed}")
                                            .monospace()
                                            .strong()
                                            .color(theme::PANEL_ALT),
                                    )
                                    .fill(theme::QUESTION),
                                )
                                .clicked()
                            {
                                open_target = Some(GuiSessionHandle {
                                    id: session.id.clone(),
                                    path: session.path.clone(),
                                });
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{91cd}\u{547d}\u{540d}")
                                            .monospace()
                                            .strong()
                                            .color(theme::PANEL_ALT),
                                    )
                                    .fill(theme::SUCCESS),
                                )
                                .clicked()
                            {
                                rename_target = Some(GuiSessionHandle {
                                    id: session.id.clone(),
                                    path: session.path.clone(),
                                });
                            }

                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{5220}\u{9664}")
                                            .monospace()
                                            .strong()
                                            .color(theme::TEXT_PRIMARY),
                                    )
                                    .fill(theme::BRICK),
                                )
                                .clicked()
                            {
                                delete_target = Some(GuiSessionHandle {
                                    id: session.id.clone(),
                                    path: session.path.clone(),
                                });
                            }
                        });
                    }
                });
                ui.add_space(6.0);
            }

            if let Some(handle) = rename_target {
                self.start_rename_session(&handle);
            }
            if let Some(handle) = rename_commit_target {
                self.commit_rename_session(handle);
            }
            if cancel_rename {
                self.renaming_session_id = None;
                self.rename_draft.clear();
            }
            if let Some(handle) = open_target {
                self.open_session(handle);
            }
            if let Some(handle) = delete_target {
                self.delete_session(handle);
            }
        });
    }

    fn render_agent_panel(&mut self, ui: &mut egui::Ui) {
        let status_color = if self.request_status.contains("\u{5931}\u{8d25}") {
            theme::ERROR
        } else if self.inflight {
            theme::WARNING
        } else {
            theme::SUCCESS
        };
        let status_label = if self.request_status.contains("\u{5931}\u{8d25}") {
            "\u{5f02}\u{5e38}"
        } else if self.inflight {
            "执行中"
        } else {
            "就绪"
        };
        hud_card(
            ui,
            "AGT",
            "Agent状态",
            Some((status_label, status_color)),
            |ui| {
                ui.label(theme::label(&format!(
                    "\u{5f53}\u{524d}\u{8fd0}\u{884c}\u{72b6}\u{6001}:{}",
                    self.request_status
                )));
                ui.label(theme::label(&format!(
                    "\u{5de5}\u{7a0b}\u{76ee}\u{5f55}:{}",
                    self.service.workspace().display()
                )));
                ui.add_space(10.0);

                if let Some(snapshot) = &self.desktop_automation {
                    theme::pixel_frame(theme::PANEL).show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "\u{684c}\u{9762}\u{4efb}\u{52a1} #{} | {}",
                                snapshot.job_id,
                                phase_label(snapshot.phase)
                            ))
                            .monospace()
                            .strong()
                            .size(15.0)
                            .color(theme::SUCCESS),
                        );
                        ui.separator();
                        ui.label(theme::label(&format!(
                            "\u{6b65}\u{9aa4}:{}/{}",
                            snapshot.step, snapshot.max_steps
                        )));
                        ui.label(theme::label(&format!(
                            "\u{6a21}\u{5f0f}:{}",
                            mode_label(snapshot.mode)
                        )));
                        ui.label(theme::label(&snapshot.detail));
                        if let Some(target_window) = &snapshot.target_window {
                            ui.label(theme::label(&format!("\u{76ee}\u{6807}:{}", target_window)));
                        }
                        if let Some(last_action) = &snapshot.last_action {
                            ui.label(theme::label(&format!("\u{52a8}\u{4f5c}:{}", last_action)));
                        }
                        if let Some(capture_path) = &snapshot.last_capture_path {
                            ui.label(theme::label(&format!(
                                "\u{622a}\u{56fe}:{}",
                                capture_path.display()
                            )));
                        }
                        if let Some(capture_size_bytes) = snapshot.last_capture_size_bytes {
                            ui.label(theme::label(&format!(
                                "\u{622a}\u{56fe}\u{5927}\u{5c0f}:{} \u{5b57}\u{8282}",
                                capture_size_bytes
                            )));
                        }
                        ui.label(theme::label(&format!(
                            "\u{7a97}\u{53e3}\u{6570}:{}",
                            snapshot.visible_windows.len()
                        )));
                    });
                    ui.add_space(10.0);
                }

                for (name, status, timeout_seconds) in [
                    (
                        "\u{4e3b}\u{4ee3}\u{7406}",
                        agent_status_label(
                            self.inflight,
                            self.active_route,
                            self.last_route,
                            AgentRoute::Chat,
                        ),
                        self.config_draft.chat_agent_timeout_seconds,
                    ),
                    (
                        "\u{89c6}\u{89c9}\u{4ee3}\u{7406}",
                        agent_status_label(
                            self.inflight,
                            self.active_route,
                            self.last_route,
                            AgentRoute::DesktopVision,
                        ),
                        self.config_draft.vision_agent_timeout_seconds,
                    ),
                    (
                        "\u{684c}\u{9762}\u{4ee3}\u{7406}",
                        agent_status_label(
                            self.inflight,
                            self.active_route,
                            self.last_route,
                            AgentRoute::DesktopAutomation,
                        ),
                        self.config_draft.vision_agent_timeout_seconds,
                    ),
                ] {
                    theme::pixel_frame(theme::PANEL).show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{name} | {status}"))
                                .monospace()
                                .strong()
                                .size(15.0)
                                .color(theme::QUESTION),
                        );
                        ui.separator();
                        ui.label(theme::label(&format!(
                            "\u{8d85}\u{65f6}\u{8bbe}\u{7f6e}:{timeout_seconds} \u{79d2}"
                        )));
                    });
                    ui.add_space(6.0);
                }
            },
        );
    }

    fn render_response_panel(&mut self, ui: &mut egui::Ui, desired_height: f32) {
        theme::pixel_frame(theme::PANEL).show(ui, |ui| {
            ui.set_min_height(desired_height.max(560.0));
            ui.label(theme::heading("\u{5bf9}\u{8bdd}\u{56de}\u{590d}\u{4e0e}\u{63a8}\u{7406}"));
            ui.separator();

            let rendered_messages = self
                .messages
                .iter()
                .filter(|message| message.role != GuiRole::System)
                .collect::<Vec<_>>();
            let total_messages = rendered_messages.len();
            let visible_count = self.visible_message_count.min(total_messages);
            let start_index = total_messages.saturating_sub(visible_count);

            if start_index > 0 {
                ui.horizontal_centered(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("\u{52a0}\u{8f7d}\u{66f4}\u{65e9}\u{5185}\u{5bb9}")
                                    .monospace()
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            )
                            .fill(theme::BRICK),
                        )
                        .clicked()
                    {
                        self.visible_message_count =
                            (visible_count + MESSAGE_LOAD_STEP).min(total_messages);
                    }
                });
                ui.add_space(10.0);
            }

            ScrollArea::vertical()
                .id_salt("response-scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for message in rendered_messages.into_iter().skip(start_index) {
                        let (title, fill) = match message.role {
                            GuiRole::User => ("\u{7528}\u{6237}", theme::USER_BUBBLE),
                            GuiRole::Assistant => ("\u{52a9}\u{624b}", theme::ASSISTANT_BUBBLE),
                            GuiRole::System => ("", theme::PANEL_ALT),
                        };
                        theme::pixel_frame(fill).show(ui, |ui| {
                            ui.label(
                                RichText::new(title)
                                    .monospace()
                                    .strong()
                                    .size(15.0)
                                    .color(theme::QUESTION),
                            );
                            ui.separator();
                            ui.label(
                                RichText::new(&message.text)
                                    .monospace()
                                    .size(15.0)
                                    .color(theme::TEXT_PRIMARY),
                            );
                        });
                        ui.add_space(10.0);
                    }

                    if self.inflight {
                        theme::pixel_frame(theme::PANEL_ALT).show(ui, |ui| {
                            ui.label(
                                RichText::new(
                                    "\u{52a9}\u{624b}\u{6b63}\u{5728}\u{5904}\u{7406}\u{5f53}\u{524d}\u{8bf7}\u{6c42},\u{8bf7}\u{7a0d}\u{5019}\u{2026}",
                                )
                                .monospace()
                                .size(16.0)
                                .color(theme::TEXT_MUTED),
                            );
                        });
                    }
                });
        });
    }

    fn render_inner_vision_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let status = if self.desktop_capture.texture().is_some() {
            ("已连接", theme::SUCCESS)
        } else {
            ("待采集", theme::WARNING)
        };
        hud_card(ui, "IN", "内视觉", Some(status), |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("\u{6293}\u{53d6}\u{539f}\u{56fe}")
                                .monospace()
                                .strong()
                                .color(theme::PANEL_ALT),
                        )
                        .fill(theme::QUESTION),
                    )
                    .clicked()
                {
                    match self.desktop_capture.capture_now(ctx) {
                        Ok(snapshot) => {
                            self.push_trace(
                                "\u{684c}\u{9762}\u{91c7}\u{96c6}",
                                format!(
                                    "\u{5df2}\u{6293}\u{53d6}\u{6700}\u{65b0}\u{539f}\u{59cb}\u{5206}\u{8fa8}\u{7387}\u{622a}\u{56fe}:{}",
                                    snapshot.path.display()
                                ),
                            );
                        }
                        Err(error) => {
                            self.push_trace(
                                "\u{684c}\u{9762}\u{91c7}\u{96c6}",
                                format!("\u{6293}\u{53d6}\u{5931}\u{8d25}:{error}"),
                            );
                        }
                    }
                }
            });
            ui.add_space(6.0);

            if let Some(texture) = self.desktop_capture.texture() {
                let available = ui.available_width().max(280.0);
                let texture_size = texture.size_vec2();
                let scale = (available / texture_size.x).min(1.0);
                let desired = Vec2::new(texture_size.x * scale, texture_size.y * scale);
                ui.add(egui::Image::from_texture(texture).fit_to_exact_size(desired));
            } else {
                theme::pixel_frame(theme::PANEL_ALT).show(ui, |ui| {
                    ui.label(theme::label("\u{6682}\u{65e0}\u{56fe}\u{50cf}"));
                });
            }

            if let Some(frame) = self.desktop_capture.latest_frame() {
                let resolution = format!(
                    "{}x{}",
                    frame.source_dimensions.0, frame.source_dimensions.1
                );
                ui.label(theme::label(&format!("分辨率 {resolution}")))
                    .on_hover_text(format!(
                        "面板 {}x{} | {} 字节 | {}",
                        frame.display_dimensions.0,
                        frame.display_dimensions.1,
                        frame.file_size,
                        format_system_time(frame.captured_at),
                    ));
            }

            if let Some(error) = self.desktop_capture.last_error() {
                ui.label(theme::label(&format!(
                    "\u{91c7}\u{96c6}\u{544a}\u{8b66}:{error}"
                )));
            }
        });
    }

    fn render_outer_vision_panel(&self, ui: &mut egui::Ui) {
        hud_card(
            ui,
            "OUT",
            "外视觉",
            Some(("预留中", theme::WARNING)),
            |ui| {
                theme::compact_frame(theme::PANEL).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(28.0);
                        ui.label(
                            RichText::new("\u{9884}\u{7559}")
                                .monospace()
                                .strong()
                                .size(18.0)
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add_space(28.0);
                    });
                });
            },
        );
    }

    fn render_tool_panel(&mut self, ui: &mut egui::Ui) {
        hud_card(
            ui,
            "TLS",
            "工具调用",
            Some(("正常", theme::SUCCESS)),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::label("\u{8c03}\u{7528}\u{8d85}\u{65f6}(s)"));
                    ui.add(
                        egui::DragValue::new(&mut self.config_draft.tool_timeout_seconds)
                            .range(10..=900)
                            .speed(1.0),
                    );
                });
                ui.add_space(8.0);

                for (name, status) in [
                    ("CLI", "在线"),
                    ("Skill", "可用 8/8"),
                    ("插件", "可用 4/4"),
                    ("视觉分析", "就绪"),
                    ("摄像头", "预留"),
                ] {
                    theme::pixel_frame(theme::PANEL).show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{name} | {status}"))
                                .monospace()
                                .strong()
                                .size(15.0)
                                .color(theme::QUESTION),
                        );
                    });
                    ui.add_space(6.0);
                }
            },
        );
    }

    fn render_right_hud(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ScrollArea::vertical()
            .id_salt("right-sidebar-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.render_inner_vision_panel(ui, ctx);
                ui.add_space(10.0);
                self.render_outer_vision_panel(ui);
                ui.add_space(10.0);
                self.render_tool_panel(ui);
                ui.add_space(10.0);
                self.render_test_lab_panel(ui);
            });
    }

    fn render_test_lab_panel(&mut self, ui: &mut egui::Ui) {
        hud_card(ui, "TST", "测试实验室", None, |ui| {
            ui.label(theme::label("分辨率"));
            egui::ComboBox::from_id_salt("test-resolution-combo")
                .selected_text(self.test_resolution.clone())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for resolution in ["1920x1080", "1600x900", "1366x768", "1280x720"] {
                        ui.selectable_value(
                            &mut self.test_resolution,
                            resolution.to_string(),
                            resolution,
                        );
                    }
                });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(pixel_button("左键", theme::QUESTION, theme::PANEL_ALT))
                    .clicked()
                {
                    self.push_trace("测试实验室", "已触发左键测试入口。");
                }
                if ui
                    .add(pixel_button("右键", theme::QUESTION, theme::PANEL_ALT))
                    .clicked()
                {
                    self.push_trace("测试实验室", "已触发右键测试入口。");
                }
            });
            if ui
                .add(pixel_button(
                    "组合键测试",
                    theme::QUESTION,
                    theme::PANEL_ALT,
                ))
                .clicked()
            {
                self.push_trace("测试实验室", "已触发左右键组合测试入口。");
            }
            if ui
                .add(pixel_button(
                    "鼠标移动测试",
                    theme::QUESTION,
                    theme::PANEL_ALT,
                ))
                .clicked()
            {
                self.push_trace("测试实验室", "已触发鼠标移动测试入口。");
            }
        });
    }

    fn render_bottom_bar(&mut self, ui: &mut egui::Ui) {
        theme::hud_frame(theme::PANEL).show(ui, |ui| {
            let max_width = ui.available_width();
            let total_width = (max_width * 0.88).clamp(720.0, max_width);
            let button_width = 112.0;
            let small_button_width = 74.0;
            let input_width =
                (total_width - button_width - small_button_width * 3.0 - 42.0).max(320.0);

            ui.vertical_centered(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(total_width, 54.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let input = ui.add_sized(
                            [input_width, 54.0],
                            TextEdit::singleline(&mut self.composer)
                                .hint_text("输入消息，按 Enter 发送..."),
                        );
                        let send_clicked = ui
                            .add_enabled(
                                !self.inflight,
                                egui::Button::new(
                                    RichText::new("\u{53d1}\u{9001}")
                                        .monospace()
                                        .strong()
                                        .size(18.0)
                                        .color(theme::PANEL_ALT),
                                )
                                .fill(theme::QUESTION)
                                .min_size(Vec2::new(button_width, 54.0)),
                            )
                            .clicked();

                        if ui
                            .add_sized(
                                [small_button_width, 54.0],
                                pixel_button("朗读", theme::QUESTION, theme::PANEL_ALT),
                            )
                            .clicked()
                        {
                            self.push_trace("语音", "模型回复朗读入口已预留。");
                        }

                        if ui
                            .add_sized(
                                [small_button_width, 54.0],
                                pixel_button("听写", theme::QUESTION, theme::PANEL_ALT),
                            )
                            .clicked()
                        {
                            self.push_trace("语音", "本地语音转文本入口已预留。");
                        }

                        if ui
                            .add_sized(
                                [small_button_width, 54.0],
                                pixel_button("框选", theme::QUESTION, theme::PANEL_ALT),
                            )
                            .clicked()
                        {
                            self.push_trace("区域视觉", "框选监控入口已预留。");
                        }

                        if send_clicked
                            || (input.lost_focus()
                                && ui.input(|input| input.key_pressed(egui::Key::Enter)))
                        {
                            self.submit();
                        }
                    },
                );
            });
        });
    }

    fn render_config_window(&mut self, ctx: &egui::Context) {
        if !self.show_config_panel {
            return;
        }

        let mut open = self.show_config_panel;
        egui::Window::new("\u{914d}\u{7f6e}\u{4e2d}\u{5fc3}")
            .default_width(620.0)
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                theme::pixel_frame(theme::PANEL).show(ui, |ui| {
                    ui.label(theme::heading("\u{754c}\u{9762}\u{914d}\u{7f6e}"));
                    ui.separator();
                    let provider_before = self.config_draft.provider.clone();
                    provider_combo(
                        ui,
                        "\u{63d0}\u{4f9b}\u{65b9}",
                        &mut self.config_draft.provider,
                    );
                    if self.config_draft.provider != provider_before {
                        if let Some(option) =
                            api::provider_option(&self.config_draft.provider)
                        {
                            self.config_draft.api_base_url = option.default_base_url.to_string();
                            if let Some(model) = option.recommended_models.first() {
                                self.config_draft.chat_model = (*model).to_string();
                            }
                        }
                    }
                    model_combo(
                        ui,
                        "\u{804a}\u{5929}\u{6a21}\u{578b}",
                        &self.config_draft.provider,
                        &mut self.config_draft.chat_model,
                    );
                    config_field(
                        ui,
                        "\u{89c6}\u{89c9}\u{6a21}\u{578b}",
                        &mut self.config_draft.vision_model,
                        "glm-vision",
                    );
                    config_field(
                        ui,
                        "\u{89c6}\u{89c9}\u{540e}\u{7aef}",
                        &mut self.config_draft.vision_backend,
                        "local-openai | zhipu",
                    );
                    config_field(
                        ui,
                        "\u{672c}\u{5730}\u{89c6}\u{89c9}\u{6a21}\u{578b}",
                        &mut self.config_draft.local_vision_model,
                        "qwen2.5-vl-3b",
                    );
                    config_field(
                        ui,
                        "\u{672c}\u{5730}\u{89c6}\u{89c9}\u{63a5}\u{53e3}",
                        &mut self.config_draft.local_vision_base_url,
                        "http://127.0.0.1:8001/v1",
                    );
                    config_field(
                        ui,
                        "\u{672c}\u{5730}\u{89c6}\u{89c9} API Key",
                        &mut self.config_draft.local_vision_api_key,
                        "\u{53ef}\u{9009}",
                    );
                    ui.checkbox(
                        &mut self.config_draft.cloud_vision_fallback_enabled,
                        "\u{672c}\u{5730}\u{89c6}\u{89c9}\u{5931}\u{8d25}\u{65f6}\u{5141}\u{8bb8}\u{8fdc}\u{7aef}\u{89c6}\u{89c9}\u{515c}\u{5e95}",
                    );
                    config_field(
                        ui,
                        "\u{5de5}\u{4f5c}\u{76ee}\u{5f55}",
                        &mut self.config_draft.workspace,
                        "C:\\path\\to\\workspace",
                    );
                    config_field(
                        ui,
                        "API Key \u{6587}\u{4ef6}\u{8def}\u{5f84}",
                        &mut self.config_draft.api_key_path,
                        "C:\\Users\\<you>\\Desktop\\api-key.txt",
                    );
                    config_field(
                        ui,
                        "\u{63a5}\u{53e3}\u{5730}\u{5740}",
                        &mut self.config_draft.api_base_url,
                        "https://open.bigmodel.cn/api/paas/v4",
                    );
                    config_field(
                        ui,
                        "\u{4ee3}\u{7406}\u{6863}\u{6848}",
                        &mut self.config_draft.agent_profile,
                        "coolzhu-dev",
                    );
                    config_field(
                        ui,
                        "\u{4e0a}\u{4e0b}\u{6587}\u{5f15}\u{64ce}",
                        &mut self.config_draft.context_engine,
                        "focused | full | minimal",
                    );
                    ui.checkbox(
                        &mut self.config_draft.fast_mode,
                        "\u{542f}\u{7528}\u{6781}\u{901f}\u{6a21}\u{5f0f}",
                    );
                    config_field(
                        ui,
                        "\u{8f93}\u{5165}\u{6ce8}\u{5165}\u{540e}\u{7aef}",
                        &mut self.config_draft.mouse_injection_backend,
                        "sendinput | interception",
                    );
                    config_field(
                        ui,
                        "Interception DLL \u{8def}\u{5f84}",
                        &mut self.config_draft.interception_dll_path,
                        "C:\\tools\\interception\\interception.dll",
                    );
                    numeric_field(
                        ui,
                        "Interception \u{9f20}\u{6807}\u{8bbe}\u{5907} ID",
                        &mut self.config_draft.interception_mouse_device_id,
                        11..=20,
                    );
                    numeric_field(
                        ui,
                        "Interception \u{952e}\u{76d8}\u{8bbe}\u{5907} ID",
                        &mut self.config_draft.interception_keyboard_device_id,
                        1..=10,
                    );

                    ui.add_space(10.0);
                    numeric_field(
                        ui,
                        "\u{4e3b}\u{4ee3}\u{7406}\u{8d85}\u{65f6}(s)",
                        &mut self.config_draft.chat_agent_timeout_seconds,
                        5..=900,
                    );
                    numeric_field(
                        ui,
                        "\u{89c6}\u{89c9}\u{4ee3}\u{7406}\u{8d85}\u{65f6}(s)",
                        &mut self.config_draft.vision_agent_timeout_seconds,
                        5..=900,
                    );
                    numeric_field(
                        ui,
                        "\u{5de5}\u{5177}\u{8c03}\u{7528}\u{8d85}\u{65f6}(s)",
                        &mut self.config_draft.tool_timeout_seconds,
                        10..=900,
                    );

                    ui.add_space(12.0);
                    ui.label(theme::heading("\u{5bfc}\u{5165} JSON"));
                    ui.separator();
                    config_field(
                        ui,
                        "JSON \u{6587}\u{4ef6}\u{8def}\u{5f84}",
                        &mut self.config_import_path,
                        &self.demo_config_path.display().to_string(),
                    );

                    ui.add_space(14.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{4fdd}\u{5b58}\u{5e76}\u{5e94}\u{7528}")
                                        .monospace()
                                        .strong()
                                        .color(theme::PANEL_ALT),
                                )
                                .fill(theme::QUESTION),
                            )
                            .clicked()
                        {
                            self.save_and_apply_config();
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{4ece}\u{78c1}\u{76d8}\u{91cd}\u{8f7d}")
                                        .monospace()
                                        .strong()
                                        .color(theme::TEXT_PRIMARY),
                                )
                                .fill(theme::BRICK),
                            )
                            .clicked()
                        {
                            self.reload_config();
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("\u{5bfc}\u{5165} JSON \u{5e76}\u{5e94}\u{7528}")
                                        .monospace()
                                        .strong()
                                        .color(theme::PANEL_ALT),
                                )
                                .fill(theme::SUCCESS),
                            )
                            .clicked()
                        {
                            self.import_config_file();
                        }
                    });
                });
            });

        self.show_config_panel = open;
    }
}

impl eframe::App for ClawGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background_reply();
        self.poll_desktop_automation();
        self.desktop_capture.update(ctx);
        self.sync_automation_viewport(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));

        egui::TopBottomPanel::top("top-bar")
            .exact_height(82.0)
            .frame(egui::Frame::default().fill(theme::PANEL_ALT))
            .show(ctx, |ui| {
                let rect = ui.max_rect().shrink2(Vec2::new(16.0, 8.0));
                theme::paint_top_plaque(ui.painter(), rect, "COOLZHU AGENT 控制台");
            });

        egui::TopBottomPanel::bottom("composer")
            .exact_height(108.0)
            .frame(egui::Frame::default().fill(theme::PANEL_ALT))
            .show(ctx, |ui| self.render_bottom_bar(ui));

        egui::SidePanel::left("left-sidebar")
            .resizable(true)
            .default_width(300.0)
            .min_width(260.0)
            .max_width(390.0)
            .frame(egui::Frame::default().fill(theme::PANEL_ALT))
            .show(ctx, |ui| self.render_left_hud(ui));

        egui::SidePanel::right("right-sidebar")
            .resizable(true)
            .default_width(340.0)
            .min_width(300.0)
            .max_width(460.0)
            .frame(egui::Frame::default().fill(theme::PANEL_ALT))
            .show(ctx, |ui| self.render_right_hud(ui, ctx));

        egui::CentralPanel::default().show(ctx, |ui| {
            let fill = ui.max_rect();
            let painter = ui.painter_at(fill);
            theme::paint_backdrop(&painter, fill, ui.input(|input| input.time) as f32);

            let hero_space = theme::logo_reserve_height(fill).clamp(220.0, 360.0);
            ui.add_space(hero_space);

            let available_width = ui.available_width();
            let response_width = if available_width >= 880.0 {
                (available_width * 0.92).clamp(880.0, available_width)
            } else {
                available_width
            };
            let response_height = ui.available_height().max(560.0);

            ui.vertical_centered(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(response_width, response_height),
                    Layout::top_down(Align::Min),
                    |ui| self.render_response_panel(ui, response_height),
                );
            });
        });

        self.render_config_window(ctx);
    }
}

fn hud_card<R>(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    status: Option<(&str, Color32)>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = theme::hud_frame(theme::PANEL_ALT).show(ui, |ui| {
        panel_header(ui, icon, title, status);
        ui.separator();
        add_contents(ui)
    });
    theme::paint_question_corner(ui.painter(), inner.response.rect);
    inner.inner
}

fn panel_header(ui: &mut egui::Ui, icon: &str, title: &str, status: Option<(&str, Color32)>) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(icon)
                .monospace()
                .strong()
                .size(15.0)
                .color(theme::QUESTION),
        );
        ui.label(
            RichText::new(title)
                .monospace()
                .strong()
                .size(18.0)
                .color(theme::TEXT_PRIMARY),
        );
        if let Some((label, color)) = status {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(label)
                        .monospace()
                        .strong()
                        .size(14.0)
                        .color(color),
                );
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), egui::Sense::hover());
                theme::status_dot(ui.painter(), rect.center(), color);
            });
        }
    });
}

fn pixel_button(label: &str, fill: Color32, text_color: Color32) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .monospace()
            .strong()
            .color(text_color),
    )
    .fill(fill)
}

fn stat_line(ui: &mut egui::Ui, label: &str, value: &str, accent: egui::Color32) {
    ui.horizontal_wrapped(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
        theme::status_dot(ui.painter(), rect.center(), accent);
        ui.label(
            RichText::new(label)
                .monospace()
                .strong()
                .size(16.0)
                .color(accent),
        );
        ui.label(
            RichText::new(value)
                .monospace()
                .size(16.0)
                .color(theme::TEXT_PRIMARY),
        );
    });
}

fn provider_combo(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.add_space(6.0);
    ui.label(theme::label(label));
    let selected = api::provider_option(value)
        .map(|option| format!("{} ({})", option.label, option.slug))
        .unwrap_or_else(|| value.clone());
    egui::ComboBox::from_id_salt("provider-preset-combo")
        .selected_text(selected)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for option in api::provider_catalog() {
                ui.selectable_value(
                    value,
                    option.slug.to_string(),
                    format!("{} ({})", option.label, option.slug),
                );
            }
        });
}

fn model_combo(ui: &mut egui::Ui, label: &str, provider: &str, value: &mut String) {
    ui.add_space(6.0);
    ui.label(theme::label(label));
    let Some(option) = api::provider_option(provider) else {
        ui.add(
            TextEdit::singleline(value)
                .desired_width(f32::INFINITY)
                .hint_text("model"),
        );
        return;
    };

    egui::ComboBox::from_id_salt("chat-model-preset-combo")
        .selected_text(value.clone())
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for model in option.recommended_models {
                ui.selectable_value(value, (*model).to_string(), *model);
            }
        });
    ui.add(
        TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text("custom model id"),
    );
}

fn config_field(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str) {
    ui.add_space(6.0);
    ui.label(theme::label(label));
    ui.add(
        TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .hint_text(hint),
    );
}

fn numeric_field<T>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) where
    T: egui::emath::Numeric,
{
    ui.add_space(6.0);
    ui.label(theme::label(label));
    ui.add(egui::DragValue::new(value).range(range).speed(1.0));
}

fn route_name(route: AgentRoute) -> &'static str {
    match route {
        AgentRoute::Chat => "\u{4e3b}\u{4ee3}\u{7406}",
        AgentRoute::DesktopVision => "\u{89c6}\u{89c9}\u{4ee3}\u{7406}",
        AgentRoute::DesktopAutomation => "\u{684c}\u{9762}\u{4ee3}\u{7406}",
    }
}

fn agent_status_label(
    inflight: bool,
    active_route: Option<AgentRoute>,
    last_route: AgentRoute,
    target_route: AgentRoute,
) -> &'static str {
    if inflight && active_route == Some(target_route) {
        "\u{6267}\u{884c}\u{4e2d}"
    } else if last_route == target_route {
        "\u{6700}\u{8fd1}\u{5df2}\u{5b8c}\u{6210}"
    } else {
        "\u{5f85}\u{547d}"
    }
}

fn format_system_time(value: std::time::SystemTime) -> String {
    value
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
