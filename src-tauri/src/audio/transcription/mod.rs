use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{path::BaseDirectory, AppHandle, Manager};

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub text: String,
    pub language: String,
    pub language_probability: f64,
    pub segments: Vec<TranscriptionSegment>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptionError {
    pub error: String,
}

// Helper to find the python executable - checks LOGIA_PYTHON_PATH first (Nix), then venv
fn get_python_executable(venv_path: &PathBuf) -> (PathBuf, bool) {
    // First check for LOGIA_PYTHON_PATH (set by Nix package - has faster-whisper bundled)
    if let Ok(path) = std::env::var("LOGIA_PYTHON_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return (p, true); // true = using system python (Nix)
        }
    }
    
    // Fallback: check if venv exists and has Python
    let venv_python = if cfg!(windows) {
        venv_path.join("Scripts").join("python.exe")
    } else {
        venv_path.join("bin").join("python")
    };
    
    if venv_python.exists() {
        return (venv_python, false); // Use venv python
    }
    
    // Otherwise return expected venv path (may not exist yet)
    let python_path = if cfg!(windows) {
        let candidates = [
            venv_path.join("Scripts").join("python.exe"),
            venv_path.join("Scripts").join("python"),
            venv_path.join("Scripts").join("python3.exe"),
            venv_path.join("Scripts").join("python3"),
        ];
        candidates.iter().find(|p| p.exists()).cloned()
            .unwrap_or_else(|| venv_path.join("Scripts").join("python.exe"))
    } else {
        venv_path.join("bin").join("python")
    };
    
    (python_path, false)
}

/// Validate a WAV file path for safe use with the transcription subprocess.
/// Rejects relative paths, parent-traversal, non-.wav extensions, and
/// paths that aren't under the app data directory (unless allowlisted).
fn validate_wav_path(wav_path: &str, app_handle: &AppHandle) -> Result<(), String> {
    if wav_path.is_empty() {
        return Err("WAV path is empty".to_string());
    }

    // Reject parent traversal
    if wav_path.contains("..") {
        return Err(format!("WAV path contains '..' (path traversal rejected): {}", wav_path));
    }

    let path = Path::new(wav_path);

    // Must be absolute
    if !path.is_absolute() {
        return Err(format!("WAV path must be absolute: {}", wav_path));
    }

    // Must have .wav / .WAV extension
    let valid_extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("wav"))
        .unwrap_or(false);
    if !valid_extension {
        return Err(format!("WAV path must end in .wav or .WAV: {}", wav_path));
    }

    // Allow paths under the app data/temp directory
    if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
        if path.starts_with(&app_data_dir) {
            return Ok(());
        }
    }

    // Also allow standard temp directories
    let temp_dir = std::env::temp_dir();
    if path.starts_with(&temp_dir) {
        return Ok(());
    }

    // Reject — path is not in an expected location
    Err(format!("WAV path is outside allowed directories (app data or temp): {}", wav_path))
}

/// Validate that the transcription script path exists and is a regular file.
fn validate_script_path(script_path: &Path) -> Result<(), String> {
    if !script_path.exists() {
        return Err(format!("Transcription script not found: {}", script_path.display()));
    }
    if !script_path.is_file() {
        return Err(format!("Transcription script path is not a file: {}", script_path.display()));
    }
    Ok(())
}

pub fn transcribe(app_handle: &AppHandle, wav_path: &str) -> Result<String, String> {
    // --- security: validate WAV path before touching the filesystem ---
    validate_wav_path(wav_path, app_handle)?;

    let app_data_dir = app_handle.path().app_data_dir().unwrap();
    let venv_path = app_data_dir.join("transcription_venv");
    let (python_path, is_system_python) = get_python_executable(&venv_path);

    // Only check venv python existence - system python from LOGIA_PYTHON_PATH is already validated
    if !is_system_python && !python_path.exists() {
        return Err(format!("Python executable not found at {:?}. Please run the transcription setup first.", python_path));
    }

    // Check for LOGIA_TRANSCRIBE_SCRIPT env var first (set by Nix package)
    let script_path = if let Ok(script) = std::env::var("LOGIA_TRANSCRIBE_SCRIPT") {
        PathBuf::from(script)
    } else if cfg!(debug_assertions) {
        PathBuf::from("src/audio/transcription/transcribe.py")
    } else {
        app_handle.path().resolve("src/audio/transcription/transcribe.py", BaseDirectory::Resource)
           .map_err(|e| format!("Failed to resolve transcribe.py: {}", e))?
    };

    // --- security: validate script path before execution ---
    validate_script_path(&script_path)?;

    // Run transcription script
    // usage: python transcribe.py <wav_path>
    // It prints JSON to stdout.
    // argv form (Command::arg) — never passes unsanitized strings to a shell.

    // Hide console on Windows
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = Command::new(&python_path);
    cmd.arg(&script_path).arg(wav_path);

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output()
        .map_err(|e| format!("Failed to execute transcription script: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Transcription script failed with code {:?}: {}", 
            output.status.code(), 
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout_str)
}