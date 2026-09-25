#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod config;
mod desktop_agent;
mod desktop_anchor;
mod desktop_capture;
mod input_backend;
mod service;
mod sessions;
mod theme;

use std::env;

use app::ClawGuiApp;
use config::GuiConfigStore;
use desktop_agent::{mode_label, phase_label, spawn_desktop_automation, DesktopAutomationEvent};
use input_backend::{click_point as input_click_point, preflight_report};
use service::{run_desktop_vision_test, run_smoke_test};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = diagnostics::init("coolzhu-agent");
    diagnostics::info("gui", "startup", "COOLZHU AGENT process started", &[]);

    let mut args = env::args().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--smoke-test" {
            let prompt = args.collect::<Vec<_>>().join(" ");
            if prompt.trim().is_empty() {
                return Err("--smoke-test requires a prompt".into());
            }
            let result = run_smoke_test(&prompt)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "mode": result.mode,
                    "model": result.model,
                    "reply": result.reply,
                    "request_id": result.request_id,
                    "total_tokens": result.total_tokens,
                }))?
            );
            return Ok(());
        }
        if flag == "--desktop-vision-test" {
            let prompt = args.collect::<Vec<_>>().join(" ");
            let prompt = if prompt.trim().is_empty() {
                "请看一下当前桌面，并简要描述你能看到什么。".to_string()
            } else {
                prompt
            };
            let result = run_desktop_vision_test(&prompt)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "mode": result.mode,
                    "model": result.model,
                    "reply": result.reply,
                    "request_id": result.request_id,
                    "total_tokens": result.total_tokens,
                }))?
            );
            return Ok(());
        }
        if flag == "--desktop-automation-test" {
            let prompt = args.collect::<Vec<_>>().join(" ");
            if prompt.trim().is_empty() {
                return Err("--desktop-automation-test requires a prompt".into());
            }

            let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
            let store = GuiConfigStore::load_or_create(&cwd)
                .unwrap_or_else(|_| GuiConfigStore::fallback(&cwd));
            let receiver = spawn_desktop_automation(store.config().clone(), prompt);
            loop {
                match receiver.recv()? {
                    DesktopAutomationEvent::Progress(snapshot) => {
                        println!(
                            "{}",
                            serde_json::to_string(&serde_json::json!({
                                "event": "progress",
                                "job_id": snapshot.job_id,
                                "mode": mode_label(snapshot.mode),
                                "phase": phase_label(snapshot.phase),
                                "step": snapshot.step,
                                "max_steps": snapshot.max_steps,
                                "detail": snapshot.detail,
                                "target_window": snapshot.target_window,
                                "last_action": snapshot.last_action,
                                "visible_windows": snapshot.visible_windows.len(),
                                "last_capture_path": snapshot.last_capture_path,
                                "last_capture_size_bytes": snapshot.last_capture_size_bytes,
                            }))?
                        );
                    }
                    DesktopAutomationEvent::Finished(result) => match result {
                        Ok(result) => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "event": "finished",
                                    "ok": true,
                                    "mode": mode_label(result.mode),
                                    "summary": result.summary,
                                    "model": result.model,
                                    "steps_completed": result.steps_completed,
                                    "target_window": result.target_window,
                                }))?
                            );
                            return Ok(());
                        }
                        Err(error) => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "event": "finished",
                                    "ok": false,
                                    "error": error,
                                }))?
                            );
                            return Err("desktop automation test failed".into());
                        }
                    },
                }
            }
        }
        if flag == "--input-backend-self-test" {
            let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
            let store = GuiConfigStore::load_or_create(&cwd)
                .unwrap_or_else(|_| GuiConfigStore::fallback(&cwd));
            store.config().apply_process_env();
            let report = preflight_report();
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        if flag == "--input-backend-click-test" {
            let x = args
                .next()
                .ok_or("--input-backend-click-test requires x")?
                .parse::<i32>()?;
            let y = args
                .next()
                .ok_or("--input-backend-click-test requires y")?
                .parse::<i32>()?;
            let clicks = args
                .next()
                .map(|value| value.parse::<u32>())
                .transpose()?
                .unwrap_or(1);
            let cwd = env::current_dir().unwrap_or_else(|_| ".".into());
            let store = GuiConfigStore::load_or_create(&cwd)
                .unwrap_or_else(|_| GuiConfigStore::fallback(&cwd));
            store.config().apply_process_env();
            input_click_point(x, y, clicks, std::time::Duration::from_secs(6))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "x": x,
                    "y": y,
                    "clicks": clicks,
                    "backend": preflight_report().backend,
                }))?
            );
            return Ok(());
        }
    }

    let title = "COOLZHU AGENT\u{63a7}\u{5236}\u{53f0}";
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1180.0, 760.0])
        .with_position([48.0, 48.0])
        .with_title(title);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(|cc| {
            theme::apply_theme(&cc.egui_ctx);
            Ok(Box::new(ClawGuiApp::new()))
        }),
    )?;

    Ok(())
}

fn window_icon() -> Option<eframe::egui::IconData> {
    eframe::icon_data::from_png_bytes(
        include_bytes!("../assets/app-icon-cz-moon-gate-lantern-v1.png"),
    )
    .ok()
}
