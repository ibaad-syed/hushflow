//! Local cleanup: turns a raw transcript into the text the speaker meant to
//! type (fillers out, self-corrections applied, lists and paragraphs) with a
//! small on-device LLM.
//!
//! Dictation never waits on it. Until the chosen model is downloaded and its
//! server is warm, or when a reply is slow or fails the output guard, the
//! rule-based cleanup (filler removal, spoken "new line") is what gets pasted.

pub mod catalog;
pub mod prompt;
mod server;

use crate::settings::get_settings;
use anyhow::{anyhow, Result};
use catalog::{CleanupEngineInfo, CleanupModel, APPLE_INTELLIGENCE};
use futures_util::StreamExt;
use log::{debug, info, warn};
use serde::Serialize;
use server::LlamaServer;
use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Past this, the transcript is pasted as is. Long dictations take ~0.8 s on
/// an M4 Pro; slower machines get headroom before cleanup gives up.
const DEADLINE: Duration = Duration::from_secs(4);
const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize, Type)]
pub struct CleanupStatus {
    pub enabled: bool,
    pub engine: String,
    pub engines: Vec<CleanupEngineInfo>,
    /// The selected engine can clean up right now (downloaded and loaded).
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress<'a> {
    id: &'a str,
    downloaded: u64,
    total: u64,
}

pub struct CleanupManager {
    app: AppHandle,
    dir: PathBuf,
    http: reqwest::Client,
    server: tokio::sync::Mutex<Option<LlamaServer>>,
    /// Model id → bytes downloaded, while downloading.
    downloads: Mutex<HashMap<String, u64>>,
}

impl CleanupManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = crate::portable::app_data_dir(app)?
            .join("models")
            .join("cleanup");
        std::fs::create_dir_all(&dir)?;
        server::stop_stale(&dir.join(server::PID_FILE));
        Ok(Self {
            app: app.clone(),
            dir,
            http: reqwest::Client::new(),
            server: tokio::sync::Mutex::new(None),
            downloads: Mutex::new(HashMap::new()),
        })
    }

    fn model_path(&self, model: &CleanupModel) -> PathBuf {
        self.dir.join(model.file)
    }

    pub fn is_downloaded(&self, id: &str) -> bool {
        catalog::find(id).is_some_and(|m| self.model_path(m).exists())
    }

    pub fn status(&self) -> CleanupStatus {
        let settings = get_settings(&self.app);
        let downloads = self.downloads.lock().unwrap().clone();
        let mut engines: Vec<CleanupEngineInfo> = catalog::MODELS
            .iter()
            .map(|m| CleanupEngineInfo {
                id: m.id.into(),
                name: m.name.into(),
                description: m.description.into(),
                size_mb: m.size_mb,
                recommended: m.id == catalog::RECOMMENDED,
                downloaded: self.is_downloaded(m.id),
                downloading: downloads.get(m.id).copied(),
            })
            .collect();
        if apple_intelligence_available() {
            engines.push(CleanupEngineInfo {
                id: APPLE_INTELLIGENCE.into(),
                name: "Apple Intelligence".into(),
                description: "Built into macOS, no download".into(),
                size_mb: 0,
                recommended: false,
                downloaded: true,
                downloading: None,
            });
        }
        let ready = match settings.cleanup_engine.as_str() {
            APPLE_INTELLIGENCE => apple_intelligence_available(),
            id => self
                .server
                .try_lock()
                .ok()
                .is_some_and(|s| s.as_ref().is_some_and(|s| s.model_id == id)),
        };
        CleanupStatus {
            enabled: settings.cleanup_enabled,
            engine: settings.cleanup_engine,
            engines,
            ready,
        }
    }

    fn notify(&self) {
        let _ = self.app.emit("cleanup-status-changed", ());
    }

    /// Downloads a model in the background (single flight per model), then
    /// loads it if it's the selected engine.
    pub fn start_download(self: &Arc<Self>, id: &str) -> Result<()> {
        let model = *catalog::find(id).ok_or_else(|| anyhow!("Unknown cleanup model {id}"))?;
        if self.is_downloaded(id) {
            return Ok(());
        }
        {
            let mut downloads = self.downloads.lock().unwrap();
            if downloads.contains_key(id) {
                return Ok(());
            }
            downloads.insert(id.to_string(), 0);
        }
        self.notify();
        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let result = manager.download(&model).await;
            manager.downloads.lock().unwrap().remove(model.id);
            match result {
                Ok(()) => {
                    info!("Cleanup model '{}' downloaded", model.id);
                    let _ = manager.app.emit("cleanup-download-complete", model.id);
                    manager.warm().await;
                }
                Err(e) => {
                    warn!("Cleanup model '{}' download failed: {e}", model.id);
                    let _ = manager.app.emit("cleanup-download-failed", model.id);
                }
            }
            manager.notify();
        });
        Ok(())
    }

    async fn download(&self, model: &CleanupModel) -> Result<()> {
        let path = self.model_path(model);
        let partial = path.with_extension("gguf.partial");
        // Resume an interrupted download instead of starting over.
        let have = tokio::fs::metadata(&partial)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = self.http.get(model.url());
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let response = request.send().await?.error_for_status()?;
        let resumed = have > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut downloaded = if resumed { have } else { 0 };
        if resumed {
            info!("Resuming cleanup model '{}' at {have} bytes", model.id);
        }
        let total = response
            .content_length()
            .map(|len| len + downloaded)
            .unwrap_or(model.size_mb * 1_000_000);
        let mut file = if resumed {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&partial)
                .await?
        } else {
            tokio::fs::File::create(&partial).await?
        };
        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
            downloaded += chunk.len() as u64;
            if last_emit.elapsed() > Duration::from_millis(250) {
                last_emit = Instant::now();
                self.downloads
                    .lock()
                    .unwrap()
                    .insert(model.id.to_string(), downloaded);
                let _ = self.app.emit(
                    "cleanup-download-progress",
                    DownloadProgress {
                        id: model.id,
                        downloaded,
                        total,
                    },
                );
            }
        }
        tokio::io::AsyncWriteExt::flush(&mut file).await?;
        drop(file);
        if downloaded < total / 2 {
            return Err(anyhow!(
                "download ended early ({downloaded} of {total} bytes)"
            ));
        }
        tokio::fs::rename(&partial, &path).await?;
        Ok(())
    }

    pub fn delete_model(&self, id: &str) -> Result<()> {
        let model = catalog::find(id).ok_or_else(|| anyhow!("Unknown cleanup model {id}"))?;
        if let Ok(mut server) = self.server.try_lock() {
            if server.as_ref().is_some_and(|s| s.model_id == id) {
                *server = None;
            }
        }
        let path = self.model_path(model);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.notify();
        Ok(())
    }

    /// At launch: fetch the chosen model if cleanup is on but it isn't on disk
    /// (a download interrupted by quitting resumes), otherwise load it.
    pub async fn resume(self: &Arc<Self>) {
        let settings = get_settings(&self.app);
        if settings.cleanup_enabled
            && catalog::find(&settings.cleanup_engine).is_some()
            && !self.is_downloaded(&settings.cleanup_engine)
        {
            if let Err(e) = self.start_download(&settings.cleanup_engine) {
                warn!("Couldn't resume cleanup model download: {e}");
            }
            return;
        }
        self.warm().await;
    }

    /// Loads the selected local model so the first dictation doesn't pay for
    /// it, and primes the prompt cache with the instructions. A no-op when
    /// cleanup is off, the engine needs no server, or it isn't downloaded.
    pub async fn warm(&self) {
        let settings = get_settings(&self.app);
        if !settings.cleanup_enabled {
            return;
        }
        let Some(model) = catalog::find(&settings.cleanup_engine) else {
            return;
        };
        if !self.is_downloaded(model.id) {
            return;
        }
        let mut guard = self.server.lock().await;
        if let Some(server) = guard.as_mut() {
            if server.model_id == model.id && server.alive() {
                return;
            }
        }
        *guard = None;
        let binary = match self.server_binary() {
            Ok(path) => path,
            Err(e) => {
                warn!("Cleanup unavailable: {e}");
                return;
            }
        };
        let pid_file = self.dir.join(server::PID_FILE);
        let mut server =
            match LlamaServer::start(&binary, &self.model_path(model), model.id, &pid_file) {
                Ok(server) => server,
                Err(e) => {
                    warn!("Cleanup server failed to start: {e}");
                    return;
                }
            };
        if let Err(e) = server.wait_ready(&self.http, SERVER_START_TIMEOUT).await {
            warn!("{e}");
            return;
        }
        let port = server.port;
        *guard = Some(server);
        drop(guard);
        let started = Instant::now();
        let _ = self.complete(port, "Warm up the cache, please.").await;
        debug!("Cleanup prompt cache primed in {:?}", started.elapsed());
        self.notify();
    }

    /// Stops the local server (cleanup turned off).
    pub async fn stop(&self) {
        *self.server.lock().await = None;
        self.notify();
    }

    /// Stops the server synchronously on app exit, so it never outlives us.
    pub fn stop_now(&self) {
        if let Ok(mut server) = self.server.try_lock() {
            *server = None;
        }
    }

    fn server_binary(&self) -> Result<PathBuf> {
        let name = if cfg!(windows) {
            "resources/llama/llama-server.exe"
        } else {
            "resources/llama/llama-server"
        };
        Ok(self
            .app
            .path()
            .resolve(name, tauri::path::BaseDirectory::Resource)?)
    }

    async fn complete(&self, port: u16, transcript: &str) -> Result<String> {
        let words = transcript.split_whitespace().count();
        let body = serde_json::json!({
            "messages": prompt::messages(transcript),
            "temperature": 0,
            "max_tokens": (words * 3 + 64).min(1500),
            "cache_prompt": true,
            "chat_template_kwargs": {"enable_thinking": false},
        });
        let reply: serde_json::Value = self
            .http
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&body)
            .timeout(DEADLINE)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        reply["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| anyhow!("cleanup reply had no text"))
    }

    /// The cleaned text, or `None` to paste the transcript as it is.
    pub async fn clean(&self, transcript: &str) -> Option<String> {
        let settings = get_settings(&self.app);
        if !settings.cleanup_enabled || !prompt::worth_cleaning(transcript) {
            return None;
        }
        let started = Instant::now();
        let reply = if settings.cleanup_engine == APPLE_INTELLIGENCE {
            apple_intelligence_clean(transcript).await
        } else {
            let port = {
                let mut guard = self.server.lock().await;
                guard
                    .as_mut()
                    .filter(|s| s.model_id == settings.cleanup_engine)
                    .and_then(|s| s.alive().then_some(s.port))
            };
            match port {
                Some(port) => self.complete(port, transcript).await,
                None => {
                    // Not loaded yet (still downloading, or the app just
                    // started): paste the rule-based text, load for next time.
                    if let Some(manager) = self.app.try_state::<Arc<CleanupManager>>() {
                        let manager = Arc::clone(&manager);
                        tauri::async_runtime::spawn(async move { manager.warm().await });
                    }
                    Err(anyhow!("cleanup engine not ready yet"))
                }
            }
        };
        match reply {
            Ok(reply) => match prompt::accept(transcript, &reply) {
                Some(cleaned) => {
                    info!("Cleanup took {:?}", started.elapsed());
                    Some(cleaned)
                }
                None => {
                    warn!("Cleanup output failed the guard; pasting the transcript");
                    None
                }
            },
            Err(e) => {
                info!("Cleanup skipped after {:?}: {e}", started.elapsed());
                None
            }
        }
    }
}

pub fn apple_intelligence_available() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return crate::apple_intelligence::check_apple_intelligence_availability();
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    false
}

async fn apple_intelligence_clean(transcript: &str) -> Result<String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let user = prompt::wrap(transcript);
        let max_tokens = (transcript.split_whitespace().count() * 3 + 64) as i32;
        let task = tauri::async_runtime::spawn_blocking(move || {
            crate::apple_intelligence::process_text_with_system_prompt(
                &prompt::single_prompt(),
                &user,
                max_tokens,
            )
        });
        match tokio::time::timeout(DEADLINE, task).await {
            Ok(Ok(result)) => result.map_err(|e| anyhow!(e)),
            Ok(Err(e)) => Err(anyhow!("Apple Intelligence task failed: {e}")),
            Err(_) => Err(anyhow!("Apple Intelligence took longer than {DEADLINE:?}")),
        }
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = transcript;
        Err(anyhow!("Apple Intelligence is only available on macOS"))
    }
}
