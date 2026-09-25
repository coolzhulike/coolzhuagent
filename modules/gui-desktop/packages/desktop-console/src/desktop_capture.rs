use std::env;
use std::fs;
use std::ops::Deref;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::imageops::FilterType;
use vision::default_latest_desktop_capture_path;

const PANEL_MAX_WIDTH: u32 = 960;
const PANEL_MAX_HEIGHT: u32 = 540;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct CachedDesktopFrame {
    pub captured_at: SystemTime,
    pub source_dimensions: (u32, u32),
    pub display_dimensions: (u32, u32),
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct CapturedDesktopSnapshot {
    pub path: PathBuf,
    pub captured_at: SystemTime,
    pub source_dimensions: (u32, u32),
    pub file_size: u64,
}

impl CapturedDesktopSnapshot {
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.path.is_file()
    }

    #[must_use]
    pub fn display(&self) -> std::path::Display<'_> {
        self.path.display()
    }
}

impl Deref for CapturedDesktopSnapshot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

pub struct DesktopCaptureState {
    texture: Option<TextureHandle>,
    current_frame: Option<CachedDesktopFrame>,
    last_error: Option<String>,
    pending_reload: bool,
}

impl DesktopCaptureState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            texture: None,
            current_frame: None,
            last_error: None,
            pending_reload: true,
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        if !self.pending_reload {
            return;
        }

        self.pending_reload = false;
        if let Err(error) = self.load_latest_from_disk(ctx) {
            self.last_error = Some(error);
        }
    }

    pub fn force_refresh(&mut self) {
        self.pending_reload = true;
    }

    pub fn capture_now(&mut self, ctx: &egui::Context) -> Result<CapturedDesktopSnapshot, String> {
        let snapshot = capture_latest_desktop_snapshot_now()?;
        self.load_frame(ctx, &snapshot.path)?;
        self.last_error = None;
        Ok(snapshot)
    }

    #[must_use]
    pub fn texture(&self) -> Option<&TextureHandle> {
        self.texture.as_ref()
    }

    #[must_use]
    pub fn latest_frame(&self) -> Option<&CachedDesktopFrame> {
        self.current_frame.as_ref()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn load_latest_from_disk(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let latest_path = default_latest_desktop_capture_path();
        if !latest_path.is_file() {
            return Ok(());
        }
        self.load_frame(ctx, &latest_path)
    }

    fn load_frame(&mut self, ctx: &egui::Context, path: &Path) -> Result<(), String> {
        let metadata = fs::metadata(path).map_err(|error| {
            format!(
                "failed to read desktop capture metadata {}: {error}",
                path.display()
            )
        })?;
        let dynamic = image::open(path).map_err(|error| {
            format!(
                "failed to decode desktop capture {}: {error}",
                path.display()
            )
        })?;

        let source_dimensions = (dynamic.width(), dynamic.height());
        let preview = dynamic.resize(PANEL_MAX_WIDTH, PANEL_MAX_HEIGHT, FilterType::Triangle);
        let rgba = preview.to_rgba8();
        let display_dimensions = (rgba.width(), rgba.height());
        let image = ColorImage::from_rgba_unmultiplied(
            [display_dimensions.0 as usize, display_dimensions.1 as usize],
            rgba.as_raw(),
        );

        let texture = self.texture.get_or_insert_with(|| {
            ctx.load_texture(
                "desktop-latest-preview",
                image.clone(),
                TextureOptions::LINEAR,
            )
        });
        texture.set(image, TextureOptions::LINEAR);
        self.current_frame = Some(CachedDesktopFrame {
            captured_at: metadata.modified().unwrap_or_else(|_| SystemTime::now()),
            source_dimensions,
            display_dimensions,
            file_size: metadata.len(),
        });
        Ok(())
    }
}

pub fn capture_latest_desktop_snapshot_now() -> Result<CapturedDesktopSnapshot, String> {
    let capture_path = capture_output_path(unique_capture_sequence());
    if let Some(parent) = capture_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create desktop capture cache {}: {error}",
                parent.display()
            )
        })?;
    }

    capture_desktop_png(&capture_path)?;
    persist_latest_capture(&capture_path)?;

    let metadata = fs::metadata(&capture_path).map_err(|error| {
        format!(
            "failed to read desktop capture metadata {}: {error}",
            capture_path.display()
        )
    })?;
    let dynamic = image::open(&capture_path).map_err(|error| {
        format!(
            "failed to decode desktop capture {}: {error}",
            capture_path.display()
        )
    })?;

    Ok(CapturedDesktopSnapshot {
        path: capture_path,
        captured_at: SystemTime::now(),
        source_dimensions: (dynamic.width(), dynamic.height()),
        file_size: metadata.len(),
    })
}

fn persist_latest_capture(source: &Path) -> Result<(), String> {
    let latest_path = default_latest_desktop_capture_path();
    if let Some(parent) = latest_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create latest desktop capture directory {}: {error}",
                parent.display()
            )
        })?;
    }

    fs::copy(source, &latest_path).map_err(|error| {
        format!(
            "failed to update latest desktop capture {}: {error}",
            latest_path.display()
        )
    })?;
    Ok(())
}

fn capture_desktop_png(path: &Path) -> Result<(), String> {
    let escaped_path = path.display().to_string().replace('\'', "''");
    let script = format!(
        "$signature = @'\nusing System;\nusing System.Runtime.InteropServices;\npublic static class DpiAwareness {{\n    [DllImport(\"user32.dll\")] public static extern bool SetProcessDPIAware();\n}}\n'@; \
         Add-Type $signature; \
         [DpiAwareness]::SetProcessDPIAware() | Out-Null; \
         Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; \
         $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen; \
         $bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height; \
         $graphics = [System.Drawing.Graphics]::FromImage($bitmap); \
         $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size); \
         $bitmap.Save('{escaped_path}', [System.Drawing.Imaging.ImageFormat]::Png); \
         $graphics.Dispose(); \
         $bitmap.Dispose();"
    );

    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        &script,
    ]);

    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let output = command
        .output()
        .map_err(|error| format!("failed to start desktop capture PowerShell: {error}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Err(format!(
            "desktop capture command failed: {} {}",
            stdout, stderr
        ))
    }
}

fn capture_output_path(sequence: u64) -> PathBuf {
    env::temp_dir()
        .join("claw-gui-captures")
        .join(format!("desktop-{sequence:05}.png"))
}

fn unique_capture_sequence() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(1)
}
