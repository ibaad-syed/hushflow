//! A `llama-server` child process serving one cleanup model on localhost.
//!
//! A separate process keeps llama.cpp's ggml apart from the one transcribe-cpp
//! links, and a crash there can't take the app down. Prompt-lookup speculative
//! decoding drafts from the transcript itself, which cleanup output mostly
//! repeats; in our eval it halved the slow tail (p90 1070 → 431 ms).

use anyhow::{anyhow, Result};
use log::{info, warn};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Records the server's pid, so one left behind by a crash is stopped on the
/// next launch instead of holding gigabytes of memory.
pub const PID_FILE: &str = "llama-server.pid";

pub struct LlamaServer {
    child: Child,
    pub port: u16,
    pub model_id: String,
    pid_file: std::path::PathBuf,
}

/// Stops a server recorded in `pid_file` if it is still running (and is in
/// fact llama-server, not an unrelated process that reused the pid).
pub fn stop_stale(pid_file: &Path) {
    let Ok(pid) = std::fs::read_to_string(pid_file) else {
        return;
    };
    let _ = std::fs::remove_file(pid_file);
    let pid = pid.trim();
    #[cfg(unix)]
    {
        let name = Command::new("ps")
            .args(["-p", pid, "-o", "comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        if name.contains("llama-server") {
            warn!("Stopping cleanup server left from a previous run (pid {pid})");
            let _ = Command::new("kill").arg(pid).status();
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            // Both filters must match, so a reused pid is left alone.
            .args([
                "/F",
                "/FI",
                &format!("PID eq {pid}"),
                "/FI",
                "IMAGENAME eq llama-server.exe",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl LlamaServer {
    pub fn start(binary: &Path, model: &Path, model_id: &str, pid_file: &Path) -> Result<Self> {
        if !binary.exists() {
            return Err(anyhow!(
                "llama-server not found at {} (run `bun scripts/fetch-llama.ts`)",
                binary.display()
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755));
        }
        let port = free_port()?;
        let mut command = Command::new(binary);
        command
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .arg("-m")
            .arg(model)
            .args(["-ngl", "99", "-c", "4096", "-np", "1", "--jinja"])
            .args(["--cache-reuse", "256", "--no-webui"])
            .args(["--spec-type", "ngram-simple"])
            .args([
                "--spec-ngram-simple-size-n",
                "3",
                "--spec-ngram-simple-size-m",
                "16",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command.spawn()?;
        let _ = std::fs::write(pid_file, child.id().to_string());
        info!("Started cleanup server for '{model_id}' on port {port}");
        Ok(Self {
            child,
            port,
            model_id: model_id.to_string(),
            pid_file: pid_file.to_path_buf(),
        })
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    /// Whether the process is still running.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Waits until the model is loaded and the server answers.
    pub async fn wait_ready(&mut self, client: &reqwest::Client, timeout: Duration) -> Result<()> {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if !self.alive() {
                return Err(anyhow!("cleanup server exited while loading"));
            }
            if let Ok(r) = client.get(self.url("/health")).send().await {
                if r.status().is_success() {
                    info!("Cleanup server ready in {:?}", started.elapsed());
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        Err(anyhow!("cleanup server not ready after {timeout:?}"))
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        if let Err(e) = self.child.kill() {
            warn!("Failed to stop cleanup server: {e}");
        }
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}
