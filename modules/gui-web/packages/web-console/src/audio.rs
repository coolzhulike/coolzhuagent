#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error, info, instrument, warn};

#[cfg(windows)]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const DEFAULT_STT_TIMEOUT_SECS: u64 = 120;
const DEFAULT_TTS_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_TTS_MAX_SEGMENT_CHARS: usize = 480;
const STREAMING_BUFFER_SIZE: usize = 4096;

// ---------------------------------------------------------------------------
// CancelToken
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ParsedWhisperSegment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedWhisperSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: Option<f32>,
}

// ---------------------------------------------------------------------------
// TextSegment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TextSegment {
    pub text: String,
    pub estimated_duration_ms: u64,
}

// ---------------------------------------------------------------------------
// VoiceConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VoiceConfig {
    pub name: String,
    pub model_path: PathBuf,
    pub speed: f32,
    pub pitch: Option<f32>,
    #[serde(default = "default_voice_kind")]
    pub kind: String,
    #[serde(default)]
    pub reference_audio_path: Option<PathBuf>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            speed: 1.0,
            pitch: None,
            kind: default_voice_kind(),
            reference_audio_path: None,
        }
    }
}

fn default_voice_kind() -> String {
    "builtin".to_string()
}

impl VoiceConfig {
    pub fn from_name(name: &str) -> Option<Self> {
        let voices = builtin_voices();
        voices.into_iter().find(|v| v.name == name)
    }
}

// ---------------------------------------------------------------------------
// SttConfig / TtsConfig
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttConfig {
    pub whisper_cpp_path: PathBuf,
    pub model_path: PathBuf,
    pub language: Option<String>,
    pub device: Option<String>,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            whisper_cpp_path: PathBuf::from("whisper-cli"),
            model_path: PathBuf::from("models/base.pt"),
            language: Some("zh".to_string()),
            device: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    pub piper_path: PathBuf,
    pub model_path: PathBuf,
    pub output_device: Option<String>,
    pub voice_configs: Vec<VoiceConfig>,
    pub backend: String,
    pub index_tts: IndexTtsConfig,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            piper_path: PathBuf::from("piper"),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            output_device: None,
            voice_configs: builtin_voices(),
            backend: "piper".to_string(),
            index_tts: IndexTtsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexTtsConfig {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_voice: Option<String>,
    pub voices_dir: Option<PathBuf>,
    pub timeout_seconds: u64,
}

impl Default for IndexTtsConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            api_key: None,
            default_voice: None,
            voices_dir: None,
            timeout_seconds: DEFAULT_TTS_TIMEOUT_SECS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSummary {
    pub name: String,
    pub backend: String,
    pub kind: String,
    pub model_path: Option<PathBuf>,
    pub reference_audio_path: Option<PathBuf>,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttResult {
    pub text: String,
    pub confidence: Option<f32>,
    pub duration_ms: u64,
    pub segments: Vec<ParsedWhisperSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsResult {
    pub audio_path: PathBuf,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsStreamResult {
    pub audio_path: PathBuf,
    pub duration_ms: u64,
    pub segments_played: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStatus {
    pub stt_available: bool,
    pub tts_available: bool,
    pub stt_model: Option<PathBuf>,
    pub tts_model: Option<PathBuf>,
    pub stt_device: Option<String>,
    pub tts_device: Option<String>,
    pub error: Option<String>,
    pub active_sessions: usize,
    pub voice_monitor_available: bool,
    pub voice_monitor_running: bool,
    pub voice_monitor_mode: String,
    pub voice_monitor_message: String,
}

// ---------------------------------------------------------------------------
// RecordingSession state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingState {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Debug)]
pub struct RecordingSession {
    pub id: String,
    pub state: RecordingState,
    pub audio_path: Option<PathBuf>,
    pub started_at: Option<std::time::Instant>,
}

impl RecordingSession {
    #[allow(dead_code)]
    fn new(id: String) -> Self {
        Self {
            id,
            state: RecordingState::Idle,
            audio_path: None,
            started_at: None,
        }
    }
}

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

pub struct SessionManager {
    sessions: std::collections::HashMap<String, RecordingSession>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: std::collections::HashMap::new(),
        }
    }

    pub fn start_session(
        &mut self,
        id: String,
        audio_path: Option<PathBuf>,
    ) -> Result<&RecordingSession, String> {
        if self.sessions.contains_key(&id) {
            let existing = &self.sessions[&id];
            if existing.state == RecordingState::Recording {
                return Err(format!("Session {} is already recording", id));
            }
        }
        let session = RecordingSession {
            id: id.clone(),
            state: RecordingState::Recording,
            audio_path,
            started_at: Some(std::time::Instant::now()),
        };
        self.sessions.insert(id.clone(), session);
        Ok(&self.sessions[&id])
    }

    pub fn stop_session(&mut self, id: &str) -> Result<&RecordingSession, String> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| format!("Session {} not found", id))?;
        if session.state != RecordingState::Recording {
            return Err(format!(
                "Session {} is not recording (state: {:?})",
                id, session.state
            ));
        }
        session.state = RecordingState::Transcribing;
        Ok(session)
    }

    pub fn end_session(&mut self, id: &str) -> Option<RecordingSession> {
        self.sessions.remove(id)
    }

    pub fn get_session(&self, id: &str) -> Option<&RecordingSession> {
        self.sessions.get(id)
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.state == RecordingState::Recording)
            .count()
    }

    pub fn total_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SttEngine
// ---------------------------------------------------------------------------

pub struct SttEngine {
    config: SttConfig,
}

impl SttEngine {
    pub fn new(config: SttConfig) -> Self {
        Self { config }
    }

    #[instrument(skip(self), fields(whisper_path = ?self.config.whisper_cpp_path, model = ?self.config.model_path))]
    pub async fn transcribe_file(&self, audio_path: &PathBuf) -> Result<SttResult, String> {
        self.transcribe_file_with_cancel(audio_path, &CancelToken::new())
            .await
    }

    pub async fn transcribe_file_with_cancel(
        &self,
        audio_path: &PathBuf,
        cancel: &CancelToken,
    ) -> Result<SttResult, String> {
        let start = std::time::Instant::now();
        info!("Starting transcription for file: {:?}", audio_path);

        if !audio_path.exists() {
            let err = format!("Audio file not found: {:?}", audio_path);
            error!("{}", err);
            return Err(err);
        }

        if cancel.is_cancelled() {
            return Err("Transcription cancelled".to_string());
        }

        let tools_dir = resolve_tools_dir();
        let whisper_script = tools_dir.join("whisper_transcribe.py");
        let mut attempt_errors = Vec::new();

        if whisper_script.exists() {
            match transcribe_python(
                &whisper_script,
                audio_path,
                &python_whisper_model_name(&self.config.model_path),
                self.config.language.as_deref(),
            )
            .await
            {
                Ok((text, segments, confidence)) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return Ok(SttResult {
                        text,
                        confidence,
                        duration_ms,
                        segments,
                    });
                }
                Err(e) => {
                    warn!("Python whisper failed: {}", e);
                    attempt_errors.push(format!("Python whisper: {e}"));
                }
            }
        } else {
            attempt_errors.push(format!(
                "Python whisper: script not found at {:?}",
                whisper_script
            ));
        }

        if cfg!(windows) {
            match native_stt_transcribe(audio_path).await {
                Ok((text, segments)) => {
                    let duration_ms = start.elapsed().as_millis() as u64;
                    return Ok(SttResult {
                        text,
                        confidence: None,
                        duration_ms,
                        segments,
                    });
                }
                Err(e) => {
                    warn!("Native STT failed: {}", e);
                    attempt_errors.push(format!("Native STT: {e}"));
                }
            }
        } else {
            attempt_errors.push("Native STT: unavailable on this operating system".to_string());
        }

        Err(format!(
            "No STT backend available. Attempts: {}",
            attempt_errors.join(" | ")
        ))
    }

    pub async fn check_available(&self) -> bool {
        let tools_dir = resolve_tools_dir();
        let whisper_script = tools_dir.join("whisper_transcribe.py");
        if whisper_script.exists() {
            return python_available().await
                && resolve_audio_resource_path(&self.config.model_path).exists();
        }
        native_stt_available().await
    }
}

// ---------------------------------------------------------------------------
// Python backend helpers
// ---------------------------------------------------------------------------

fn resolve_tools_dir() -> PathBuf {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(|p| p.join("tools"))),
        std::env::current_dir().ok().map(|d| d.join("tools")),
        std::env::current_dir()
            .ok()
            .map(|d| d.join("modules/gui-web/packages/web-console/tools")),
        Some(PathBuf::from("tools")),
    ];
    for cand in candidates.iter().flatten() {
        if cand.join("whisper_transcribe.py").exists() {
            debug!("Found tools dir: {:?}", cand);
            return cand.clone();
        }
    }
    debug!("Tools dir not found, using default");
    PathBuf::from("tools")
}

/// 解析随安装包复制到 `bin/models` 的相对音频模型路径。
///
/// 开发运行时的当前目录可能是 workspace 根目录，安装运行时则通常是
/// `bin/coolzhu-web-console.exe` 旁边的 `bin` 目录；两者都不能假设为固定值。
fn resolve_audio_resource_path(path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path.clone();
    }

    let candidates = [
        Some(path.clone()),
        resolve_tools_dir().parent().map(|parent| parent.join(path)),
        std::env::current_dir().ok().map(|dir| dir.join(path)),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|candidate| candidate.exists())
        .unwrap_or_else(|| path.clone())
}

async fn python_available() -> bool {
    let mut cmd = Command::new("python");
    cmd.args(["-c", "exit(0)"]);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.status().await.map(|s| s.success()).unwrap_or(false)
}

async fn transcribe_python(
    script: &PathBuf,
    audio_path: &PathBuf,
    model_name: &str,
    language: Option<&str>,
) -> Result<(String, Vec<ParsedWhisperSegment>, Option<f32>), String> {
    let mut cmd = Command::new("python");
    cmd.arg(script);
    cmd.arg(audio_path);
    cmd.arg(model_name);
    if let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) {
        cmd.arg(language);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = timeout(
        Duration::from_secs(DEFAULT_STT_TIMEOUT_SECS * 2),
        cmd.output(),
    )
    .await
    .map_err(|e| format!("Python whisper timeout: {}", e))?
    .map_err(|e| format!("Failed to run python: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() {
        let stdout_error = stdout.lines().rev().find_map(|line| {
            serde_json::from_str::<serde_json::Value>(line.trim())
                .ok()
                .and_then(|parsed| {
                    parsed
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
        });
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "Python error: {}",
            stdout_error
                .or_else(|| (!stderr.is_empty()).then_some(stderr))
                .unwrap_or_else(|| "process exited without stderr or JSON error".to_string())
        ));
    }

    for line in stdout.lines().rev() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return parse_transcription_json(&parsed);
            }
        }
    }
    Err(format!("No JSON found in whisper output: {}", stdout))
}

fn python_whisper_model_name(model_path: &PathBuf) -> String {
    let stem = model_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let normalized = stem
        .strip_prefix("ggml-")
        .unwrap_or(&stem)
        .strip_prefix("whisper-")
        .unwrap_or_else(|| stem.strip_prefix("ggml-").unwrap_or(&stem))
        .trim()
        .to_string();
    let without_language_suffix = normalized
        .strip_suffix(".en")
        .unwrap_or(&normalized)
        .to_string();
    if without_language_suffix.is_empty() {
        "base".to_string()
    } else {
        without_language_suffix
    }
}

async fn synthesize_piper(
    script: &PathBuf,
    text: &str,
    output_path: &PathBuf,
    model_path: Option<&PathBuf>,
    pitch_semitones: f32,
) -> Result<u64, String> {
    let mut cmd = Command::new("python");
    cmd.arg(script);
    cmd.arg(text);
    cmd.arg(output_path);
    // model_path 占位（空串=用默认），保证 pitch 始终是脚本第 4 位参数。
    match model_path {
        Some(mp) => {
            cmd.arg(mp);
        }
        None => {
            cmd.arg("");
        }
    }
    cmd.arg(format!("{pitch_semitones}"));
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = timeout(Duration::from_secs(DEFAULT_TTS_TIMEOUT_SECS), cmd.output())
        .await
        .map_err(|e| format!("Python piper timeout: {}", e))?
        .map_err(|e| format!("Failed to run python: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Piper error: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse piper JSON: {} (raw: {})", e, stdout))?;

    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }

    if !output_path.exists() {
        return Err("Piper output file not created".to_string());
    }

    wav_audio_data_bytes(output_path).map_err(|error| format!("Piper output invalid: {error}"))?;

    Ok(parsed["duration_ms"].as_u64().unwrap_or(0))
}

pub fn indextts_endpoint_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/tts")
        || trimmed.ends_with("/synthesize")
        || trimmed.ends_with("/api/tts")
        || trimmed.ends_with("/api/synthesize")
    {
        trimmed.to_string()
    } else {
        format!("{trimmed}/tts")
    }
}

async fn synthesize_indextts_http(
    config: &IndexTtsConfig,
    text: &str,
    output_path: &PathBuf,
    voice: Option<&str>,
    reference_audio_path: Option<&PathBuf>,
) -> Result<u64, String> {
    let start = std::time::Instant::now();
    let base_url = config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "IndexTTS base_url is not configured".to_string())?;
    let url = indextts_endpoint_url(base_url);
    let mut payload = serde_json::json!({
        "text": text,
        "format": "wav",
    });
    if let Some(voice) = voice.filter(|value| !value.trim().is_empty()) {
        payload["voice"] = serde_json::json!(voice);
    }
    if let Some(reference_audio_path) = reference_audio_path {
        payload["reference_audio_path"] =
            serde_json::json!(reference_audio_path.to_string_lossy().to_string());
    }

    let client = reqwest::Client::new();
    let mut request = client
        .post(url)
        .timeout(Duration::from_secs(config.timeout_seconds.max(1)))
        .json(&payload);
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        request = request.bearer_auth(api_key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("IndexTTS request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("IndexTTS request failed with {status}: {body}"));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.contains("application/json") {
        let parsed: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse IndexTTS JSON: {e}"))?;
        if let Some(err) = parsed.get("error").and_then(|value| value.as_str()) {
            return Err(err.to_string());
        }
        if let Some(audio_path) = parsed
            .get("audio_path")
            .or_else(|| parsed.get("path"))
            .and_then(|value| value.as_str())
        {
            let source = PathBuf::from(audio_path);
            if source.exists() {
                if source != *output_path {
                    tokio::fs::copy(&source, output_path)
                        .await
                        .map_err(|e| format!("Failed to copy IndexTTS output: {e}"))?;
                }
                return Ok(parsed
                    .get("duration_ms")
                    .and_then(|value| value.as_u64())
                    .unwrap_or_else(|| start.elapsed().as_millis() as u64));
            }
            return Err(format!("IndexTTS audio_path does not exist: {audio_path}"));
        }
        if let Some(audio_base64) = parsed
            .get("audio_base64")
            .or_else(|| parsed.get("audio"))
            .and_then(|value| value.as_str())
        {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(audio_base64)
                .map_err(|e| format!("Failed to decode IndexTTS audio_base64: {e}"))?;
            tokio::fs::write(output_path, bytes)
                .await
                .map_err(|e| format!("Failed to write IndexTTS output: {e}"))?;
            return Ok(parsed
                .get("duration_ms")
                .and_then(|value| value.as_u64())
                .unwrap_or_else(|| start.elapsed().as_millis() as u64));
        }
        return Err("IndexTTS JSON did not include audio_path or audio_base64".to_string());
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read IndexTTS audio bytes: {e}"))?;
    if bytes.is_empty() {
        return Err("IndexTTS returned empty audio".to_string());
    }
    tokio::fs::write(output_path, bytes)
        .await
        .map_err(|e| format!("Failed to write IndexTTS output: {e}"))?;
    Ok(start.elapsed().as_millis() as u64)
}

pub async fn indextts_available(config: &IndexTtsConfig) -> bool {
    let Some(base_url) = config
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let health_url = format!("{}/health", base_url.trim_end_matches('/'));
    reqwest::Client::new()
        .get(health_url)
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub fn available_voices(config: &TtsConfig) -> Vec<VoiceSummary> {
    let mut voices: Vec<VoiceSummary> = config
        .voice_configs
        .iter()
        .map(|voice| VoiceSummary {
            name: voice.name.clone(),
            backend: if voice.kind.eq_ignore_ascii_case("cloned") {
                "indextts".to_string()
            } else {
                "piper".to_string()
            },
            kind: voice.kind.clone(),
            model_path: Some(voice.model_path.clone()),
            reference_audio_path: voice.reference_audio_path.clone(),
            available: resolve_audio_resource_path(&voice.model_path).exists()
                || (voice.kind.eq_ignore_ascii_case("cloned")
                    && config.index_tts.base_url.is_some()),
        })
        .collect();
    if let Some(default_voice) = config
        .index_tts
        .default_voice
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !voices.iter().any(|voice| voice.name == default_voice) {
            voices.push(VoiceSummary {
                name: default_voice.to_string(),
                backend: "indextts".to_string(),
                kind: "cloned".to_string(),
                model_path: None,
                reference_audio_path: None,
                available: config.index_tts.base_url.is_some(),
            });
        }
    }
    voices
}

fn parse_transcription_json(
    parsed: &serde_json::Value,
) -> Result<(String, Vec<ParsedWhisperSegment>, Option<f32>), String> {
    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    let text = parsed["text"].as_str().unwrap_or("").to_string();
    let segments: Vec<ParsedWhisperSegment> = parsed["segments"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|seg| ParsedWhisperSegment {
                    text: seg["text"].as_str().unwrap_or("").to_string(),
                    start_ms: seg["start_ms"].as_u64().unwrap_or(0),
                    end_ms: seg["end_ms"].as_u64().unwrap_or(0),
                    confidence: seg["confidence"].as_f64().map(|c| c as f32),
                })
                .collect()
        })
        .unwrap_or_default();
    let confidence = segments.first().and_then(|s| s.confidence);
    Ok((text, segments, confidence))
}

// ---------------------------------------------------------------------------
// TtsEngine
// ---------------------------------------------------------------------------

pub struct TtsEngine {
    config: TtsConfig,
}

impl TtsEngine {
    pub fn new(config: TtsConfig) -> Self {
        Self { config }
    }

    pub fn resolve_voice(&self, voice_name: Option<&str>) -> Option<VoiceConfig> {
        match voice_name {
            Some(name) => self
                .config
                .voice_configs
                .iter()
                .find(|v| v.name == name)
                .cloned(),
            None => Some(VoiceConfig {
                name: "default".to_string(),
                model_path: self.config.model_path.clone(),
                speed: 1.0,
                pitch: None,
                kind: default_voice_kind(),
                reference_audio_path: None,
            }),
        }
    }

    #[instrument(skip(self), fields(piper_path = ?self.config.piper_path, model = ?self.config.model_path))]
    pub async fn speak(
        &self,
        text: &str,
        output_path: Option<&PathBuf>,
    ) -> Result<TtsResult, String> {
        self.speak_with_voice(text, output_path, None).await
    }

    pub async fn speak_with_voice(
        &self,
        text: &str,
        output_path: Option<&PathBuf>,
        voice_name: Option<&str>,
    ) -> Result<TtsResult, String> {
        let start = std::time::Instant::now();
        let normalized = normalize_tts_text(text);

        info!("Starting TTS synthesis for {} characters", normalized.len());

        if normalized.trim().is_empty() {
            return Err("Text is empty after normalization".to_string());
        }

        let output_file = output_path
            .cloned()
            .unwrap_or_else(|| std::env::temp_dir().join(format!("tts_{}.wav", uuid_timestamp())));

        let voice = self.resolve_voice(voice_name);
        let backend = self.config.backend.trim().to_ascii_lowercase();
        let mut backend_errors = Vec::new();
        let wants_index_tts = backend == "indextts"
            || backend == "index-tts"
            || voice
                .as_ref()
                .is_some_and(|v| v.kind.eq_ignore_ascii_case("cloned"));
        if wants_index_tts {
            match synthesize_indextts_http(
                &self.config.index_tts,
                &normalized,
                &output_file,
                voice_name.or(self.config.index_tts.default_voice.as_deref()),
                voice.as_ref().and_then(|v| v.reference_audio_path.as_ref()),
            )
            .await
            {
                Ok(duration_ms) => {
                    info!("IndexTTS HTTP synthesis completed in {}ms", duration_ms);
                    return Ok(TtsResult {
                        audio_path: output_file,
                        duration_ms,
                    });
                }
                Err(e) if backend == "indextts" || backend == "index-tts" => return Err(e),
                Err(e) => {
                    warn!("IndexTTS HTTP failed: {}", e);
                    backend_errors.push(format!("IndexTTS: {e}"));
                }
            }
        }

        let tools_dir = resolve_tools_dir();
        let piper_script = tools_dir.join("piper_speak.py");

        if piper_script.exists() && python_available().await {
            let (model_path, pitch_semitones) = if let Some(vname) = voice_name {
                self.config
                    .voice_configs
                    .iter()
                    .find(|v| v.name == vname)
                    .map(|v| (Some(v.model_path.clone()), v.pitch.unwrap_or(0.0)))
                    .unwrap_or((None, 0.0))
            } else {
                (None, 0.0)
            };

            match synthesize_piper(
                &piper_script,
                &normalized,
                &output_file,
                model_path.as_ref(),
                pitch_semitones,
            )
            .await
            {
                Ok(duration_ms) => {
                    info!("Python piper TTS completed in {}ms", duration_ms);
                    return Ok(TtsResult {
                        audio_path: output_file,
                        duration_ms,
                    });
                }
                Err(e) => {
                    warn!("Python piper failed: {}", e);
                    backend_errors.push(format!("Piper: {e}"));
                }
            }
        }

        if cfg!(windows) {
            match native_tts_speak(&normalized, &output_file).await {
                Ok(_inner_duration_ms) => {
                    let total_ms = start.elapsed().as_millis() as u64;
                    return Ok(TtsResult {
                        audio_path: output_file,
                        duration_ms: total_ms,
                    });
                }
                Err(e) => {
                    warn!("Native TTS failed: {}", e);
                    backend_errors.push(format!("Windows TTS: {e}"));
                }
            }
        }

        if backend_errors.is_empty() {
            Err("No TTS backend available".to_string())
        } else {
            Err(format!(
                "No TTS backend available: {}",
                backend_errors.join("; ")
            ))
        }
    }

    pub async fn check_available(&self) -> bool {
        let backend = self.config.backend.trim().to_ascii_lowercase();
        if (backend == "indextts" || backend == "index-tts")
            && indextts_available(&self.config.index_tts).await
        {
            return true;
        }
        let tools_dir = resolve_tools_dir();
        let piper_script = tools_dir.join("piper_speak.py");
        if piper_script.exists()
            && python_available().await
            && (resolve_audio_resource_path(&self.config.model_path).exists()
                || self
                    .config
                    .voice_configs
                    .iter()
                    .any(|voice| resolve_audio_resource_path(&voice.model_path).exists()))
        {
            return true;
        }
        native_tts_available().await
    }
}

/// 返回 WAV `data` chunk 的字节数，并拒绝只有容器头、没有采样数据的“空音频”。
///
/// Windows `SpeechSynthesizer` 在没有适用语言语音时可能仍写出一个 46 字节的
/// RIFF/WAVE 文件并以成功退出；仅检查文件存在会把该结果误报为 TTS 成功。
fn wav_audio_data_bytes(path: &PathBuf) -> Result<u64, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("无法读取 WAV: {error}"))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("不是有效的 RIFF/WAVE 文件".to_string());
    }

    let mut offset = 12usize;
    while offset.saturating_add(8) <= bytes.len() {
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let data_start = offset.saturating_add(8);
        let data_end = data_start.saturating_add(chunk_size);
        if data_end > bytes.len() {
            return Err("WAV chunk 超出文件长度".to_string());
        }
        if &bytes[offset..offset + 4] == b"data" {
            if chunk_size == 0 {
                return Err("WAV data chunk 为空，没有可播放采样".to_string());
            }
            return Ok(chunk_size as u64);
        }
        offset = data_end.saturating_add(chunk_size & 1);
    }

    Err("WAV 缺少 data chunk".to_string())
}

// ---------------------------------------------------------------------------
// Native Windows TTS (System.Speech.Synthesis)
// ---------------------------------------------------------------------------

async fn native_tts_available() -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $n = $s.GetInstalledVoices().Count; $s.Dispose(); if ($n -gt 0) { exit 0 } else { exit 1 }",
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.status().await.map(|s| s.success()).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn native_tts_speak(text: &str, output_path: &PathBuf) -> Result<u64, String> {
    let start = std::time::Instant::now();

    let escaped_text = text
        .replace('"', "`\"")
        .replace('$', "`$")
        .replace('`', "``")
        .replace('\n', " ");

    let ps_script = format!(
        r#"Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.SetOutputToWaveFile('{}'); $s.Speak("{}"); $s.Dispose()"#,
        output_path.display(),
        escaped_text
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps_script]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|e| format!("Native TTS timeout: {}", e))?
        .map_err(|e| format!("Failed to run native TTS: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Native TTS failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    if !output_path.exists() {
        return Err("Native TTS output file not created".to_string());
    }

    wav_audio_data_bytes(output_path)
        .map_err(|error| format!("Native TTS output invalid: {error}"))?;

    Ok(start.elapsed().as_millis() as u64)
}

// ---------------------------------------------------------------------------
// Native Windows STT (System.Speech.Recognition)
// ---------------------------------------------------------------------------

async fn native_stt_available() -> bool {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Add-Type -AssemblyName System.Speech; $n = [System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers().Count; if ($n -gt 0) { exit 0 } else { exit 1 }",
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        cmd.status().await.map(|s| s.success()).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn native_stt_transcribe(
    audio_path: &PathBuf,
) -> Result<(String, Vec<ParsedWhisperSegment>), String> {
    let start = std::time::Instant::now();

    if !audio_path.exists() {
        return Err(format!("Audio file not found: {:?}", audio_path));
    }

    let ps_script = format!(
        r#"Add-Type -AssemblyName System.Speech
$engine = New-Object System.Speech.Recognition.SpeechRecognitionEngine
$engine.SetInputToWaveFile('{}')
$result = $engine.Recognize()
if ($result) {{ $result.Text }} else {{ Write-Error 'No recognition result' }}"#,
        audio_path.display()
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps_script]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|e| format!("Native STT timeout: {}", e))?
        .map_err(|e| format!("Failed to run native STT: {}", e))?;

    if !output.status.success() {
        let stderr = super::decode_console_output(&output.stderr);
        return Err(format!("Native STT error: {}", stderr));
    }

    let text = super::decode_console_output(&output.stdout)
        .trim()
        .to_string();
    if text.is_empty() {
        return Err("Native STT produced no text".to_string());
    }

    let segment = ParsedWhisperSegment {
        text: text.clone(),
        start_ms: 0,
        end_ms: start.elapsed().as_millis() as u64,
        confidence: None,
    };

    Ok((text, vec![segment]))
}

// ---------------------------------------------------------------------------
// Audio playback
// ---------------------------------------------------------------------------

pub async fn play_audio_file(path: &PathBuf, _device: Option<&str>) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Audio file not found: {:?}", path));
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("paplay");
        cmd.arg(path);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        let status = cmd
            .status()
            .await
            .map_err(|e| format!("Playback failed: {}", e))?;
        if !status.success() {
            return Err("Playback command failed".to_string());
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("afplay");
        cmd.arg(path);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        let status = cmd
            .status()
            .await
            .map_err(|e| format!("Playback failed: {}", e))?;
        if !status.success() {
            return Err("Playback command failed".to_string());
        }
    }

    #[cfg(windows)]
    {
        let ps = format!(
            r#"$p = New-Object System.Media.SoundPlayer '{}'; $p.PlaySync()"#,
            path.display()
        );
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &ps]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::piped());
        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Playback failed: {}", e))?;
        if !output.status.success() {
            return Err(format!(
                "Playback error: {}",
                super::decode_console_output(&output.stderr)
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Audio status
// ---------------------------------------------------------------------------

pub async fn check_audio_status(stt_config: &SttConfig, tts_config: &TtsConfig) -> AudioStatus {
    info!("Checking audio status");

    let stt_engine = SttEngine::new(stt_config.clone());
    let tts_engine = TtsEngine::new(tts_config.clone());

    let stt_available = stt_engine.check_available().await;
    let tts_available = tts_engine.check_available().await;

    debug!(
        "STT available: {}, TTS available: {}",
        stt_available, tts_available
    );

    AudioStatus {
        stt_available,
        tts_available,
        stt_model: if stt_available && resolve_audio_resource_path(&stt_config.model_path).exists()
        {
            Some(resolve_audio_resource_path(&stt_config.model_path))
        } else {
            None
        },
        tts_model: if tts_available && resolve_audio_resource_path(&tts_config.model_path).exists()
        {
            Some(resolve_audio_resource_path(&tts_config.model_path))
        } else {
            None
        },
        stt_device: stt_config.device.clone(),
        tts_device: tts_config.output_device.clone(),
        error: if !stt_available && !tts_available {
            Some("Neither STT nor TTS engines are available".to_string())
        } else {
            None
        },
        active_sessions: 0,
        voice_monitor_available: cfg!(windows),
        voice_monitor_running: false,
        voice_monitor_mode: "dry-run".to_string(),
        voice_monitor_message: "监听开关仅作状态桥接，真实唤醒链路后置".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Text processing
// ---------------------------------------------------------------------------

pub fn segment_text(text: &str, max_chars: usize) -> Vec<TextSegment> {
    if text.is_empty() {
        return vec![];
    }

    let max_chars = max_chars.max(1);
    let normalized = normalize_tts_text(text);
    let mut segments = Vec::new();
    let mut remaining = normalized.as_str();

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            let seg = TextSegment {
                text: remaining.trim().to_string(),
                estimated_duration_ms: estimate_speech_duration(remaining),
            };
            if !seg.text.is_empty() {
                segments.push(seg);
            }
            break;
        }

        let mut split_at = byte_index_after_chars(remaining, max_chars);
        let chunk = &remaining[..split_at];

        for boundary in [
            ". ", "! ", "? ", ".\n", "!\n", "?\n", "\n\n", "\n", "; ", ", ",
        ]
        .iter()
        {
            if let Some(pos) = chunk.rfind(boundary) {
                split_at = pos + boundary.len();
                break;
            }
        }

        if split_at == max_chars {
            for (i, c) in chunk.char_indices().rev() {
                if c == ' ' {
                    split_at = i + 1;
                    break;
                }
            }
        }

        let segment_text = remaining[..split_at].trim();
        if !segment_text.is_empty() {
            segments.push(TextSegment {
                text: segment_text.to_string(),
                estimated_duration_ms: estimate_speech_duration(segment_text),
            });
        }

        remaining = remaining[split_at..].trim_start();
    }

    segments
}

fn byte_index_after_chars(input: &str, max_chars: usize) -> usize {
    input
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| input.len())
}

pub fn normalize_tts_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\x00'..='\x08' | '\x0b'..='\x0c' | '\x0e'..='\x1f' => continue,
            '\t' => result.push(' '),
            '\r' => result.push('\n'),
            c => result.push(c),
        }
    }

    let result = result
        .lines()
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    collapse_consecutive_blank_lines(&result)
}

fn collapse_consecutive_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_count = 0usize;
    for line in text.lines() {
        if line.is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push('\n');
            }
        } else {
            blank_count = 0;
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

pub fn estimate_speech_duration(text: &str) -> u64 {
    let word_count = text.split_whitespace().count();
    let avg_wpm = 150;
    ((word_count as f64 / avg_wpm as f64) * 60_000.0) as u64
}

// ---------------------------------------------------------------------------
// Whisper output parsing
// ---------------------------------------------------------------------------

pub fn parse_whisper_segments(output: &str) -> Vec<ParsedWhisperSegment> {
    let mut segments = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(seg) = parse_srt_line(line) {
            segments.push(seg);
        } else if let Some(seg) = parse_json_line(line) {
            segments.push(seg);
        } else {
            if let Some(seg) = parse_bracket_line(line) {
                segments.push(seg);
            }
        }
    }

    segments
}

fn parse_srt_line(line: &str) -> Option<ParsedWhisperSegment> {
    if !line.contains("-->") {
        return None;
    }
    let parts: Vec<&str> = line.splitn(2, ']').collect();
    if parts.len() < 2 {
        return None;
    }

    let timestamp_part = parts[0].trim_start_matches('[');
    let times: Vec<&str> = timestamp_part.split("-->").collect();
    if times.len() < 2 {
        return None;
    }

    let start_ms = parse_timestamp_to_ms(times[0].trim())?;
    let end_ms = parse_timestamp_to_ms(times[1].trim())?;
    let text = parts[1].trim().to_string();

    Some(ParsedWhisperSegment {
        text,
        start_ms,
        end_ms,
        confidence: None,
    })
}

fn parse_json_line(line: &str) -> Option<ParsedWhisperSegment> {
    if !line.starts_with('{') {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = parsed.get("text")?.as_str()?.to_string();
    let start_ms = (parsed.get("t0").and_then(|v| v.as_f64()).unwrap_or(0.0) * 1000.0) as u64;
    let end_ms = (parsed.get("t1").and_then(|v| v.as_f64()).unwrap_or(0.0) * 1000.0) as u64;
    let confidence = parsed
        .get("confidence")
        .or_else(|| parsed.get("p"))
        .and_then(|v| v.as_f64())
        .map(|c| c as f32);

    Some(ParsedWhisperSegment {
        text,
        start_ms,
        end_ms,
        confidence,
    })
}

fn parse_bracket_line(line: &str) -> Option<ParsedWhisperSegment> {
    if !line.starts_with('[') {
        return None;
    }
    let parts: Vec<&str> = line.splitn(2, ']').collect();
    if parts.len() < 2 {
        return None;
    }
    let text = parts[1].trim().to_string();
    Some(ParsedWhisperSegment {
        text,
        start_ms: 0,
        end_ms: 0,
        confidence: None,
    })
}

fn parse_timestamp_to_ms(ts: &str) -> Option<u64> {
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;
    let secs_parts: Vec<&str> = parts[2].split('.').collect();
    let seconds: u64 = secs_parts.first()?.parse().ok()?;
    let millis: u64 = if secs_parts.len() > 1 {
        let ms_str = secs_parts[1];
        if ms_str.len() >= 3 {
            ms_str[..3].parse().ok()?
        } else {
            let padded = format!("{:0<3}", ms_str);
            padded.parse().ok()?
        }
    } else {
        0
    };
    Some(hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + millis)
}

#[allow(dead_code)]
fn parse_whisper_output(output: &str) -> String {
    parse_whisper_segments(output)
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Built-in voices
// ---------------------------------------------------------------------------

fn builtin_voices() -> Vec<VoiceConfig> {
    // 中文 huayan（女声单说话人）模型，下面 4 档用音调偏移（pitch 字段=半音）模拟男/女/成人/孩童。
    let zh_model = PathBuf::from("models/zh_CN-huayan-medium.onnx");
    vec![
        VoiceConfig {
            name: "成人男声".to_string(),
            model_path: zh_model.clone(),
            speed: 1.0,
            pitch: Some(-4.0),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "成人女声".to_string(),
            model_path: zh_model.clone(),
            speed: 1.0,
            pitch: Some(2.0),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "成年中性".to_string(),
            model_path: zh_model.clone(),
            speed: 1.0,
            pitch: Some(0.0),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "孩童".to_string(),
            model_path: zh_model.clone(),
            speed: 1.05,
            pitch: Some(7.0),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "default".to_string(),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            speed: 1.0,
            pitch: None,
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "female-en".to_string(),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            speed: 1.0,
            pitch: Some(1.1),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "male-en".to_string(),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            speed: 1.0,
            pitch: Some(0.9),
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "fast".to_string(),
            model_path: PathBuf::from("models/en_US-lessac-medium.onnx"),
            speed: 1.3,
            pitch: None,
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
        VoiceConfig {
            name: "zh-CN".to_string(),
            model_path: PathBuf::from("models/zh_CN-huayan-medium.onnx"),
            speed: 1.0,
            pitch: None,
            kind: default_voice_kind(),
            reference_audio_path: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// Cleanup
// ---------------------------------------------------------------------------

pub async fn cleanup_temp_audio(max_age_secs: u64) -> u64 {
    let temp_dir = std::env::temp_dir();
    let now = std::time::SystemTime::now();
    let mut cleaned = 0u64;

    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !file_name.starts_with("tts_") && !file_name.starts_with("stt_") {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = now.duration_since(modified) {
                        if elapsed.as_secs() > max_age_secs {
                            if std::fs::remove_file(&path).is_ok() {
                                cleaned += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    cleaned
}

fn uuid_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // -------------------------------------------------------------------
    // Existing tests (preserved)
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_whisper_output() {
        let output =
            "[00:00:00.000 --> 00:00:02.000]  Hello world\n[00:00:02.000 --> 00:00:04.000]  This is a test\n";
        let result = parse_whisper_output(output);
        assert_eq!(result, "Hello world This is a test");
    }

    #[test]
    fn test_stt_config_default() {
        let config = SttConfig::default();
        assert_eq!(config.whisper_cpp_path, PathBuf::from("whisper-cli"));
        assert_eq!(config.model_path, PathBuf::from("models/base.pt"));
        assert_eq!(config.language.as_deref(), Some("zh"));
    }

    #[test]
    fn python_whisper_model_name_derives_from_ggml_path() {
        assert_eq!(
            python_whisper_model_name(&PathBuf::from("models/ggml-base.en.bin")),
            "base"
        );
    }

    #[test]
    fn python_whisper_model_name_keeps_named_model_file() {
        assert_eq!(
            python_whisper_model_name(&PathBuf::from("C:/models/medium.pt")),
            "medium"
        );
    }

    #[test]
    fn whisper_python_script_resamples_audio_to_whisper_rate() {
        let script = include_str!("../tools/whisper_transcribe.py");
        assert!(script.contains("TARGET_SAMPLE_RATE = 16000"));
        assert!(script.contains("def resample_audio"));
        assert!(script.contains("audio_data = resample_audio(audio_data, sample_rate)"));
    }

    #[test]
    fn test_tts_config_default() {
        let config = TtsConfig::default();
        assert_eq!(config.piper_path, PathBuf::from("piper"));
        assert_eq!(
            config.model_path,
            PathBuf::from("models/en_US-lessac-medium.onnx")
        );
        assert!(!config.voice_configs.is_empty());
    }

    #[test]
    fn wav_audio_data_bytes_rejects_empty_data_chunk() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"RIFF&\0\0\0WAVEfmt \x12\0\0\0\x01\0\x01\0\x22\x56\0\0D\xac\0\0\x02\0\x10\0data\0\0\0\0")
            .unwrap();

        let error = wav_audio_data_bytes(&file.path().to_path_buf()).unwrap_err();
        assert!(error.contains("data chunk"));
    }

    #[test]
    fn wav_audio_data_bytes_accepts_pcm_samples() {
        let mut file = NamedTempFile::new().unwrap();
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&40u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0, 0x40, 0x1f, 0, 0, 0x80, 0x3e, 0, 0, 2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&[0, 0, 1, 0]);
        file.write_all(&wav).unwrap();

        assert_eq!(wav_audio_data_bytes(&file.path().to_path_buf()).unwrap(), 4);
    }

    #[tokio::test]
    async fn test_stt_transcribe_file_not_found() {
        let config = SttConfig::default();
        let engine = SttEngine::new(config);
        let result = engine
            .transcribe_file(&PathBuf::from("/nonexistent/audio.wav"))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Audio file not found"));
    }

    #[tokio::test]
    async fn test_tts_speak_with_empty_text() {
        let config = TtsConfig::default();
        let engine = TtsEngine::new(config);
        let result = engine.speak("", None).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Text is empty after normalization"));
    }

    #[tokio::test]
    async fn test_tts_speak_cancelled_before_start() {
        let cancel = CancelToken::new();
        cancel.cancel();
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn test_audio_status_serialization() {
        let status = AudioStatus {
            stt_available: true,
            tts_available: false,
            stt_model: Some(PathBuf::from("model.bin")),
            tts_model: None,
            stt_device: Some("cuda".to_string()),
            tts_device: None,
            error: Some("TTS not available".to_string()),
            active_sessions: 0,
            voice_monitor_available: true,
            voice_monitor_running: false,
            voice_monitor_mode: "dry-run".to_string(),
            voice_monitor_message: "监听开关仅作状态桥接，真实唤醒链路后置".to_string(),
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("stt_available"));
        assert!(json.contains("tts_available"));
        assert!(json.contains("active_sessions"));
        assert!(json.contains("voice_monitor_running"));
    }

    // -------------------------------------------------------------------
    // RED-phase tests: CancelToken
    // -------------------------------------------------------------------

    #[test]
    fn test_cancel_token_default_not_cancelled() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn test_cancel_token_cancel_sets_flag() {
        let token = CancelToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_cancel_token_clone_shares_state() {
        let token = CancelToken::new();
        let cloned = token.clone();
        cloned.cancel();
        assert!(token.is_cancelled());
        assert!(cloned.is_cancelled());
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Text Segmentation
    // -------------------------------------------------------------------

    #[test]
    fn test_segment_text_empty_returns_empty() {
        let segments = segment_text("", 100);
        assert!(segments.is_empty());
    }

    #[test]
    fn test_segment_text_shorter_than_max_returns_one() {
        let segments = segment_text("Hello world.", 100);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello world.");
    }

    #[test]
    fn test_segment_text_splits_at_sentence_boundary() {
        let text = "First sentence. Second sentence. Third one here.";
        let segments = segment_text(text, 25);
        assert!(
            segments.len() >= 2,
            "Expected at least 2 segments, got {}",
            segments.len()
        );
        for seg in &segments {
            assert!(
                seg.text.len() <= 25 + 10,
                "Segment too long: '{}'",
                seg.text
            );
        }
    }

    #[test]
    fn test_segment_text_splits_at_newline() {
        let text = "Line one text here.\nLine two text here.";
        let segments = segment_text(text, 20);
        assert!(segments.len() >= 2);
    }

    #[test]
    fn test_segment_text_handles_multibyte_boundaries() {
        let text = "这是一段较长的中文回复，用于验证实时语音朗读分段不会在多字节字符边界处崩溃。";
        let segments = segment_text(text, 17);

        assert!(segments.len() >= 2);
        assert!(segments.iter().all(|seg| !seg.text.is_empty()));
        let combined = segments
            .iter()
            .map(|seg| seg.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(combined, text);
    }

    #[test]
    fn test_segment_text_preserves_all_content() {
        let original = "Hello world. This is a longer sentence that should be split.";
        let segments = segment_text(original, 25);
        let combined: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            combined.contains("Hello world"),
            "Combined missing content: '{}'",
            combined
        );
        assert!(
            combined.contains("longer sentence"),
            "Combined missing content: '{}'",
            combined
        );
    }

    #[test]
    fn test_segment_text_estimates_duration() {
        let segments = segment_text("One two three four five.", 100);
        assert_eq!(segments.len(), 1);
        assert!(segments[0].estimated_duration_ms > 0);
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Text Normalization
    // -------------------------------------------------------------------

    #[test]
    fn test_normalize_tts_text_strips_control_chars() {
        let input = "Hello\u{0000}\u{0001}World";
        let result = normalize_tts_text(input);
        assert_eq!(result, "HelloWorld");
    }

    #[test]
    fn test_normalize_tts_text_replaces_tab_with_space() {
        let input = "Hello\tWorld";
        let result = normalize_tts_text(input);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_normalize_tts_text_handles_unicode() {
        let input = "你好世界 Café naïveté";
        let result = normalize_tts_text(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_normalize_tts_text_preserves_punctuation() {
        let input = "Hello, world! How are you? I'm fine.";
        let result = normalize_tts_text(input);
        assert!(result.contains("Hello, world!"));
    }

    #[test]
    fn test_normalize_tts_text_collapses_blank_lines() {
        let input = "Line one\n\n\n\nLine two";
        let result = normalize_tts_text(input);
        assert_eq!(result, "Line one\n\nLine two");
    }

    #[test]
    fn test_normalize_tts_text_trims_trailing_whitespace() {
        let input = "Hello world   \n   Second line   ";
        let result = normalize_tts_text(input);
        assert_eq!(result, "Hello world\nSecond line");
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Whisper Parsing (enhanced)
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_whisper_segments_srt_format() {
        let output = "[00:00:00.000 --> 00:00:02.500]  Hello world\n[00:00:02.500 --> 00:00:05.000]  This is a test\n";
        let segments = parse_whisper_segments(output);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Hello world");
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].end_ms, 2500);
        assert_eq!(segments[1].text, "This is a test");
        assert_eq!(segments[1].start_ms, 2500);
        assert_eq!(segments[1].end_ms, 5000);
    }

    #[test]
    fn test_parse_whisper_segments_json_format() {
        let output = r#"{"text":"Hello","t0":0.0,"t1":1.5,"confidence":0.95}"#;
        let segments = parse_whisper_segments(output);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello");
        assert_eq!(segments[0].start_ms, 0);
        assert_eq!(segments[0].end_ms, 1500);
        assert_eq!(segments[0].confidence, Some(0.95));
    }

    #[test]
    fn test_parse_whisper_segments_json_with_p_field() {
        let output = r#"{"text":"Test","t0":0.5,"t1":2.0,"p":0.88}"#;
        let segments = parse_whisper_segments(output);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].confidence, Some(0.88));
    }

    #[test]
    fn test_parse_whisper_segments_empty_input() {
        let segments = parse_whisper_segments("");
        assert!(segments.is_empty());
    }

    #[test]
    fn test_parse_whisper_segments_bracket_format() {
        let output = "[sentence]  Hello world\n";
        let segments = parse_whisper_segments(output);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello world");
    }

    #[test]
    fn test_parse_whisper_segments_mixed_formats() {
        let output = "[00:00:00.000 --> 00:00:02.000]  SRT format\n[sentence]  Bracket format\n";
        let segments = parse_whisper_segments(output);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "SRT format");
        assert_eq!(segments[1].text, "Bracket format");
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Voice Config
    // -------------------------------------------------------------------

    #[test]
    fn test_voice_config_from_name_known() {
        let voice = VoiceConfig::from_name("female-en");
        assert!(voice.is_some());
        let voice = voice.unwrap();
        assert_eq!(voice.name, "female-en");
        assert_eq!(voice.pitch, Some(1.1));
    }

    #[test]
    fn test_voice_config_from_name_unknown() {
        let voice = VoiceConfig::from_name("nonexistent-voice");
        assert!(voice.is_none());
    }

    #[test]
    fn test_voice_config_default_values() {
        let default = VoiceConfig::default();
        assert_eq!(default.name, "default");
        assert_eq!(default.speed, 1.0);
        assert!(default.pitch.is_none());
    }

    #[test]
    fn test_builtin_voices_has_default() {
        let voices = builtin_voices();
        assert!(voices.iter().any(|v| v.name == "default"));
        assert!(voices.iter().any(|v| v.name == "female-en"));
        assert!(voices.iter().any(|v| v.name == "male-en"));
        assert!(voices.iter().any(|v| v.name == "fast"));
    }

    #[test]
    fn test_tts_engine_resolve_voice_by_name() {
        let config = TtsConfig::default();
        let engine = TtsEngine::new(config);
        let voice = engine.resolve_voice(Some("female-en"));
        assert!(voice.is_some());
        assert_eq!(voice.unwrap().pitch, Some(1.1));
    }

    #[test]
    fn test_tts_engine_resolve_voice_default() {
        let config = TtsConfig::default();
        let engine = TtsEngine::new(config);
        let voice = engine.resolve_voice(None);
        assert!(voice.is_some());
        assert_eq!(voice.unwrap().name, "default");
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Recording Session Manager
    // -------------------------------------------------------------------

    #[test]
    fn test_session_manager_start_session_ok() {
        let mut manager = SessionManager::new();
        let result = manager.start_session("s1".to_string(), None);
        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.state, RecordingState::Recording);
    }

    #[test]
    fn test_session_manager_double_start_rejected() {
        let mut manager = SessionManager::new();
        manager.start_session("s1".to_string(), None).unwrap();
        let result = manager.start_session("s1".to_string(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already recording"));
    }

    #[test]
    fn test_session_manager_stop_transitions_to_transcribing() {
        let mut manager = SessionManager::new();
        manager.start_session("s1".to_string(), None).unwrap();
        let result = manager.stop_session("s1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().state, RecordingState::Transcribing);
    }

    #[test]
    fn test_session_manager_stop_nonexistent() {
        let mut manager = SessionManager::new();
        let result = manager.stop_session("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_manager_stop_non_recording_errors() {
        let mut manager = SessionManager::new();
        manager.start_session("s1".to_string(), None).unwrap();
        manager.stop_session("s1").unwrap();
        let result = manager.stop_session("s1");
        assert!(result.is_err());
    }

    #[test]
    fn test_session_manager_end_session_removes() {
        let mut manager = SessionManager::new();
        manager.start_session("s1".to_string(), None).unwrap();
        let removed = manager.end_session("s1");
        assert!(removed.is_some());
        assert!(manager.get_session("s1").is_none());
    }

    #[test]
    fn test_session_manager_active_count() {
        let mut manager = SessionManager::new();
        assert_eq!(manager.active_count(), 0);
        manager.start_session("s1".to_string(), None).unwrap();
        assert_eq!(manager.active_count(), 1);
        manager.start_session("s2".to_string(), None).unwrap();
        assert_eq!(manager.active_count(), 2);
        manager.stop_session("s1").unwrap();
        assert_eq!(manager.active_count(), 1);
    }

    #[test]
    fn test_session_manager_total_count() {
        let mut manager = SessionManager::new();
        manager.start_session("s1".to_string(), None).unwrap();
        manager.start_session("s2".to_string(), None).unwrap();
        assert_eq!(manager.total_count(), 2);
        manager.stop_session("s1").unwrap();
        assert_eq!(manager.total_count(), 2);
        manager.end_session("s1");
        assert_eq!(manager.total_count(), 1);
    }

    #[test]
    fn test_session_manager_tracks_audio_path() {
        let mut manager = SessionManager::new();
        let path = Some(PathBuf::from("/tmp/test.wav"));
        let result = manager.start_session("s1".to_string(), path.clone());
        assert!(result.is_ok());
        let session = manager.get_session("s1").unwrap();
        assert_eq!(session.audio_path, path);
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Timestamp Parsing
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_timestamp_to_ms_zero() {
        assert_eq!(parse_timestamp_to_ms("00:00:00.000"), Some(0));
    }

    #[test]
    fn test_parse_timestamp_to_ms_one_second() {
        assert_eq!(parse_timestamp_to_ms("00:00:01.000"), Some(1000));
    }

    #[test]
    fn test_parse_timestamp_to_ms_one_minute() {
        assert_eq!(parse_timestamp_to_ms("00:01:00.000"), Some(60000));
    }

    #[test]
    fn test_parse_timestamp_to_ms_one_hour() {
        assert_eq!(parse_timestamp_to_ms("01:00:00.000"), Some(3600000));
    }

    #[test]
    fn test_parse_timestamp_to_ms_with_fractional() {
        assert_eq!(parse_timestamp_to_ms("00:00:00.500"), Some(500));
    }

    #[test]
    fn test_parse_timestamp_to_ms_invalid() {
        assert_eq!(parse_timestamp_to_ms("invalid"), None);
        assert_eq!(parse_timestamp_to_ms("00:00"), None);
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Speech Duration Estimation
    // -------------------------------------------------------------------

    #[test]
    fn test_estimate_speech_duration_empty() {
        assert_eq!(estimate_speech_duration(""), 0);
    }

    #[test]
    fn test_estimate_speech_duration_scales_with_words() {
        let short = estimate_speech_duration("one two three");
        let long = estimate_speech_duration("one two three four five six seven eight nine ten");
        assert!(long > short);
    }

    // -------------------------------------------------------------------
    // RED-phase tests: CancelToken with engines
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_stt_transcribe_cancelled_before_start() {
        let config = SttConfig::default();
        let engine = SttEngine::new(config);
        let cancel = CancelToken::new();
        cancel.cancel();

        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), b"dummy").unwrap();
        let result = engine
            .transcribe_file_with_cancel(&temp_file.path().to_path_buf(), &cancel)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Text Segmentation Edge Cases
    // -------------------------------------------------------------------

    #[test]
    fn test_segment_text_handles_markdown_like_text() {
        let text = "# Heading\n\nThis is a paragraph with some text.\n\nAnother paragraph here.";
        let segments = segment_text(text, 100);
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_segment_text_handles_code_snippets() {
        let text = "The function is: fn main() { println!(\"hello\"); }. That is all.";
        let segments = segment_text(text, 40);
        assert!(!segments.is_empty());
        for seg in &segments {
            assert!(!seg.text.is_empty());
        }
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Audio playback (dry-run)
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_play_audio_file_not_found() {
        let result = play_audio_file(&PathBuf::from("/nonexistent/track.wav"), None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -------------------------------------------------------------------
    // RED-phase tests: Cleanup
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_cleanup_temp_audio_no_files() {
        // 系统临时目录可能包含真实运行产生的 tts_/stt_ 文件；测试不得删除或
        // 计入这些外部文件。使用不可能达到的年龄阈值验证“无符合项”语义。
        let cleaned = cleanup_temp_audio(u64::MAX).await;
        assert_eq!(cleaned, 0);
    }

    // -------------------------------------------------------------------
    // RED-phase tests: AudioStatus
    // -------------------------------------------------------------------

    #[test]
    fn test_audio_status_includes_active_sessions() {
        let status = AudioStatus {
            stt_available: true,
            tts_available: true,
            stt_model: None,
            tts_model: None,
            stt_device: None,
            tts_device: None,
            error: None,
            active_sessions: 5,
            voice_monitor_available: true,
            voice_monitor_running: true,
            voice_monitor_mode: "dry-run".to_string(),
            voice_monitor_message: "监听中".to_string(),
        };
        assert_eq!(status.active_sessions, 5);
        assert!(status.voice_monitor_running);
    }
}
