//! Cleanup engines the user can pick: small instruct models run locally by
//! llama.cpp, and Apple's on-device model where macOS offers it.

use serde::Serialize;
use specta::Type;

/// Engine id for Apple's Foundation Models (no download).
pub const APPLE_INTELLIGENCE: &str = "apple-intelligence";
/// The engine onboarding recommends; it led our cleanup eval (28/29, ~220 ms).
pub const RECOMMENDED: &str = "gemma-4-e2b";

#[derive(Debug, Clone, Copy)]
pub struct CleanupModel {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub repo: &'static str,
    pub file: &'static str,
    pub size_mb: u64,
}

pub const MODELS: &[CleanupModel] = &[
    CleanupModel {
        id: "gemma-4-e2b",
        name: "Gemma 4 E2B",
        description: "Best balance of quality and speed",
        repo: "unsloth/gemma-4-E2B-it-GGUF",
        file: "gemma-4-E2B-it-Q4_K_M.gguf",
        size_mb: 3110,
    },
    CleanupModel {
        id: "qwen3.5-2b",
        name: "Qwen3.5 2B",
        description: "Smallest download, a little less careful",
        repo: "unsloth/Qwen3.5-2B-GGUF",
        file: "Qwen3.5-2B-Q4_K_M.gguf",
        size_mb: 1281,
    },
    CleanupModel {
        id: "qwen3.5-4b",
        name: "Qwen3.5 4B",
        description: "Neatest lists and paragraphs, slower",
        repo: "unsloth/Qwen3.5-4B-GGUF",
        file: "Qwen3.5-4B-Q4_K_M.gguf",
        size_mb: 2741,
    },
];

pub fn find(id: &str) -> Option<&'static CleanupModel> {
    MODELS.iter().find(|m| m.id == id)
}

impl CleanupModel {
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repo, self.file
        )
    }
}

/// One choice as the UI shows it.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CleanupEngineInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 0 for engines that need no download.
    pub size_mb: u64,
    pub recommended: bool,
    pub downloaded: bool,
    /// Bytes so far while downloading.
    pub downloading: Option<u64>,
}
