use chrono;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::Manager;
use fs2::FileExt;

mod audio;
mod git_sync;
mod notes;
mod config_crypto;

use notes::*;
use config_crypto::*;

/// Atomically write content to a file using a temporary file and rename.
/// This prevents data loss during concurrent writes and is safe across filesystem operations.
pub(crate) fn atomic_write_file(path: &std::path::Path, content: &str) -> Result<(), String> {
    let temp_path = path.with_extension("tmp");
    
    // Write to temp file first
    let mut temp_file = std::fs::File::create(&temp_path)
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    
    // Get exclusive lock on temp file
    temp_file.lock_exclusive()
        .map_err(|e| format!("Failed to lock file: {}", e))?;
    
    // Write content
    use std::io::Write;
    temp_file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write to temp file: {}", e))?;
    
    // Sync to disk
    temp_file.sync_all()
        .map_err(|e| format!("Failed to sync file: {}", e))?;
    
    // Unlock (happens automatically when file is dropped, but explicit is clearer)
    temp_file.unlock()
        .map_err(|e| format!("Failed to unlock file: {}", e))?;
    drop(temp_file);
    
    // Atomically rename temp to target (atomic on most filesystems)
    std::fs::rename(&temp_path, path)
        .map_err(|e| format!("Failed to rename temp file: {}", e))?;
    
    Ok(())
}

// Hide console windows on Windows when spawning subprocesses
#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn hide_console(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW
        cmd.creation_flags(0x08000000);
    }
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn start_recording(app_handle: tauri::AppHandle) -> Result<(), String> {
    audio::os_capture::start_capture(&app_handle)
}

#[tauri::command]
async fn stop_recording(app_handle: tauri::AppHandle) -> Result<String, String> {
    // Stop capture and get the WAV file path
    let wav_path = audio::os_capture::stop_capture()?;
    
    // Run transcription
    let result = audio::transcription::transcribe(&app_handle, &wav_path);
    
    // Clean up the WAV file
    let _ = std::fs::remove_file(&wav_path);
    
    result
}

// Helper to find the python executable inside a venv across platforms
fn python_executable_in_venv(venv_path: &std::path::PathBuf) -> std::path::PathBuf {
    // Check for explicit override from environment (e.g. NixOS)
    if let Ok(path) = std::env::var("LOGIA_PYTHON_PATH") {
        return std::path::PathBuf::from(path);
    }

    if cfg!(windows) {
        let candidates = [
            venv_path.join("Scripts").join("python.exe"),
            venv_path.join("Scripts").join("python"),
            venv_path.join("Scripts").join("python3.exe"),
            venv_path.join("Scripts").join("python3"),
        ];
        for p in candidates.iter() {
            if p.exists() {
                return p.clone();
            }
        }
        // Default fallback
        venv_path.join("Scripts").join("python.exe")
    } else {
        let p = venv_path.join("bin").join("python");
        if p.exists() {
            return p;
        }
        let p3 = venv_path.join("bin").join("python3");
        if p3.exists() {
            return p3;
        }
        p
    }
}

async fn ensure_transcription_dependencies(app_handle: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use std::process::Command;
    use tauri::path::BaseDirectory;

    let requirements_path = app_handle.path().resolve("src/audio/transcription/requirements.txt", BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve requirements.txt resource: {}", e))?;

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    if !app_data_dir.exists() {
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    }

    // Prepare an install log that the UI can read while installation is running
    let log_path = app_data_dir.join("transcription_install.log");
    // helper to append to the log file (best-effort)
    fn append_to_log(path: &std::path::PathBuf, msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(msg.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }

    append_to_log(&log_path, &format!("[{}] Starting dependency check/install", chrono::Utc::now().to_rfc3339()));

    // Check if we are using system python override (e.g. NixOS)
    if let Ok(system_python_path) = std::env::var("LOGIA_PYTHON_PATH") {
        #[cfg(debug_assertions)]
        println!("Using LOGIA_PYTHON_PATH from environment: {}", system_python_path);
        append_to_log(&log_path, &format!("LOGIA_PYTHON_PATH set: {}", system_python_path));
        
        let python_path = std::path::PathBuf::from(&system_python_path);
        
        // Check if faster_whisper is already available
        let mut cmd_check = Command::new(&python_path);
        cmd_check.args(&["-c", "import faster_whisper"]);
        hide_console(&mut cmd_check);
        let check_result = cmd_check.output();
        
        if let Ok(output) = check_result {
            if output.status.success() {
                #[cfg(debug_assertions)]
                println!("faster_whisper already available in system Python");
                append_to_log(&log_path, "faster_whisper already available in system Python");
                return Ok(app_data_dir.join("system_python_override"));
            }
        }
        
        // faster_whisper not installed - try to install it using uv to a local venv
        #[cfg(debug_assertions)]
        println!("faster_whisper not found in system Python, installing to local venv...");
        append_to_log(&log_path, "faster_whisper not found, creating local venv for Nix...");
        
        let nix_venv_path = app_data_dir.join("transcription_venv");
        
        // Create venv using uv with system Python
        let mut cmd_uv_venv = Command::new("uv");
        cmd_uv_venv.args(&["venv", &nix_venv_path.to_string_lossy(), "--python", &system_python_path, "--clear"]);
        hide_console(&mut cmd_uv_venv);
        
        if let Ok(status) = cmd_uv_venv.status() {
            if status.success() {
                #[cfg(debug_assertions)]
                println!("Created venv for Nix using uv");
                append_to_log(&log_path, "Created venv for Nix using uv");
                
                // Install faster-whisper
                let mut cmd_install = Command::new("uv");
                cmd_install.args(&["pip", "install", "faster-whisper", "--python", &nix_venv_path.to_string_lossy()]);
                hide_console(&mut cmd_install);
                
                if let Ok(install_status) = cmd_install.status() {
                    if install_status.success() {
                        #[cfg(debug_assertions)]
                        println!("Successfully installed faster-whisper for Nix");
                        append_to_log(&log_path, "Successfully installed faster-whisper for Nix");
                        // Return venv path so transcription uses the venv python
                        return Ok(nix_venv_path);
                    }
                }
            }
        }
        
        append_to_log(&log_path, "Failed to set up faster-whisper for Nix system Python");
        return Err("Failed to install faster-whisper for Nix. Run: uv pip install faster-whisper".to_string());
    }

    let venv_path = app_data_dir.join("transcription_venv");

    let mut cmd_uv_check = Command::new("uv");
    cmd_uv_check.arg("--version");
    hide_console(&mut cmd_uv_check);
    let uv_available = cmd_uv_check
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    append_to_log(&log_path, &format!("uv available: {}", uv_available));

    // Python version check and venv recreation logic
    if venv_path.exists() {
        let python_bin = python_executable_in_venv(&venv_path);

        if python_bin.exists() {
            let mut cmd = Command::new(&python_bin);
            cmd.arg("--version");
            hide_console(&mut cmd);
            let version_output = cmd.output().ok();

            if let Some(output) = version_output {
                let version_str = String::from_utf8_lossy(&output.stdout);
                let version_str_err = String::from_utf8_lossy(&output.stderr);

                if version_str.contains("3.14") || version_str_err.contains("3.14") {
                    #[cfg(debug_assertions)]
                    println!("Detected Python 3.14 in venv, which is likely incompatible. Recreating venv with 3.12...");
                    append_to_log(&log_path, "Detected incompatible Python in venv, removing venv to recreate");
                    let _ = std::fs::remove_dir_all(&venv_path);
                }
            }
        } else {
            // Venv dir exists but python binary is missing? it's broken.
            #[cfg(debug_assertions)]
            println!("Venv directory exists but python binary missing. Recreating...");
            append_to_log(&log_path, "Venv broken (no python binary), removing to recreate");
            let _ = std::fs::remove_dir_all(&venv_path);
        }
    }

    // Create virtual environment if it doesn't exist
    if !venv_path.exists() {
        #[cfg(debug_assertions)]
        println!("Creating virtual environment...");
        append_to_log(&log_path, "Creating virtual environment...");

        let mut venv_created = if uv_available {
            let mut cmd_uv_venv = Command::new("uv");
            cmd_uv_venv.args(&["venv", &venv_path.to_string_lossy(), "--python", "3.12"]);
            hide_console(&mut cmd_uv_venv);
            let status = cmd_uv_venv.status().map(|s| s.success()).unwrap_or(false);
            if status {
                #[cfg(debug_assertions)]
                println!("Created venv with uv (Python 3.12)");
                append_to_log(&log_path, "Created venv with uv (Python 3.12)");
            } else {
                 #[cfg(debug_assertions)]
                 println!("uv failed to create venv");
                 append_to_log(&log_path, "uv failed to create venv");
            }
            status
        } else if cfg!(windows) {
            // On Windows prefer the `py` launcher, fallback to `python`
            let path_str = venv_path.to_string_lossy().to_string();
            let mut cmd_py = Command::new("py");
            cmd_py.args(&["-3", "-m", "venv", &path_str]);
            hide_console(&mut cmd_py);
            let created_with_py = cmd_py.status().map(|s| s.success()).unwrap_or(false);

            if created_with_py {
                #[cfg(debug_assertions)]
                println!("Created venv with py launcher");
                append_to_log(&log_path, "Created venv with py launcher");
            }

            if !created_with_py {
                let mut cmd_python = Command::new("python");
                cmd_python.args(&["-m", "venv", &path_str]);
                hide_console(&mut cmd_python);
                let created_with_python = cmd_python.status().map(|s| s.success()).unwrap_or(false);

                if created_with_python {
                    #[cfg(debug_assertions)]
                    println!("Created venv with python.exe");
                    append_to_log(&log_path, "Created venv with python.exe");
                }

                created_with_python
            } else {
                true
            }
        } else {
            false
        };

        if !venv_created {
            #[cfg(debug_assertions)]
            println!("uv failed or not available, falling back to python3...");
            append_to_log(&log_path, "uv failed/missing, falling back to python3");

            // Unix-like fallback to python3
            let mut cmd_py3 = Command::new("python3");
            cmd_py3.args(&["-m", "venv", &venv_path.to_string_lossy()]);
            hide_console(&mut cmd_py3);
            let status = cmd_py3.status().map(|s| s.success()).unwrap_or(false);

            if status {
                #[cfg(debug_assertions)]
                println!("Created venv with python3");
                append_to_log(&log_path, "Created venv with python3");
            }
            venv_created = status;
        }

        if !venv_created {
            append_to_log(&log_path, "Failed to create virtual environment");
            return Err("Failed to create virtual environment".to_string());
        } else{
            #[cfg(debug_assertions)]
            println!("Venv works gng");
        }
    }

    // Locate python executable inside venv
    let python_path = python_executable_in_venv(&venv_path);

    if !python_path.exists() {
        append_to_log(&log_path, "Python executable not found in venv after creation");
        // Debug: list directory
        if let Ok(entries) = std::fs::read_dir(venv_path.join("bin")) {
             for entry in entries {
                 if let Ok(e) = entry {
                     append_to_log(&log_path, &format!("Found in bin: {:?}", e.path()));
                 }
             }
        } else {
             append_to_log(&log_path, "Could not list bin directory (missing?)");
        }
        return Err("Python executable not found in venv after creation".to_string());
    }

    // Ensure pip/setuptools/wheel/cython and imageio-ffmpeg are available to improve build success
    // (helps avoid building C extensions like 'av' from source when possible)
    #[cfg(debug_assertions)]
    println!("Upgrading pip/setuptools/wheel and installing build helpers (cython, imageio-ffmpeg)...");
    append_to_log(&log_path, "Upgrading pip/setuptools/wheel and installing build helpers (cython, imageio-ffmpeg)...");
    let mut cmd_upgrade = Command::new(&python_path);
    cmd_upgrade.args(&["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel", "cython", "imageio-ffmpeg"]);
    hide_console(&mut cmd_upgrade);
    let _ = cmd_upgrade
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .status()
        .map(|s| if s.success() { if cfg!(debug_assertions) { println!("Build helpers installed/updated"); } } else { if cfg!(debug_assertions) { println!("Warning: failed to upgrade/install build helpers (exit code: {:?})", s.code()); } });
    
    append_to_log(&log_path, "Attempting to install build helper packages (pip upgrade etc.)");

    // Check if faster_whisper is already installed
    let mut cmd_check = Command::new(&python_path);
    cmd_check.args(&["-c", "import faster_whisper"]);
    hide_console(&mut cmd_check);
    let check_import_status = cmd_check
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .status();

    if let Ok(status) = check_import_status {
        if status.success() {
            #[cfg(debug_assertions)]
            println!("faster_whisper already installed in venv.");
            append_to_log(&log_path, "faster_whisper already installed in venv.");
            return Ok(venv_path); // Dependencies already installed, return venv_path
        }
    }

    // Install dependencies if not already installed
    #[cfg(debug_assertions)]
    println!("Installing transcription dependencies...");
    append_to_log(&log_path, "Installing transcription dependencies...");

    // Prefer uv for installs. If uv is available, use it exclusively. If uv is not present, fall back to pip routes.
    if uv_available {
        #[cfg(debug_assertions)]
        println!("uv found - installing dependencies using uv...");
        append_to_log(&log_path, "uv found - installing dependencies using uv...");
        let mut cmd_uv_install = Command::new("uv");
        cmd_uv_install.args(&["pip", "install", "-r", &requirements_path.to_string_lossy(), "--python", &venv_path.to_string_lossy()]);
        hide_console(&mut cmd_uv_install);
        let status_result = cmd_uv_install
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .status();

        match status_result {
            Ok(status) if status.success() => {
                #[cfg(debug_assertions)]
                println!("Successfully installed dependencies with uv");
                append_to_log(&log_path, "Successfully installed dependencies with uv");
                return Ok(venv_path);
            },
            Ok(status) => {
                #[cfg(debug_assertions)]
                println!("uv install failed with exit code: {:?}", status.code());
                append_to_log(&log_path, &format!("uv failed with exit code: {:?}", status.code()));
                // Do not fall back automatically when uv exists; surface the error and advise user
                return Err(format!("uv failed to install dependencies (exit code {:?}). Try running 'uv pip install -r {}'.", status.code(), requirements_path.to_string_lossy()));
            },
            Err(e) => {
                #[cfg(debug_assertions)]
                println!("Failed to execute uv: {}", e);
                append_to_log(&log_path, &format!("Failed to execute uv: {}", e));
                return Err(format!("Failed to execute uv: {}", e));
            }
        }
    } else {
        // uv not available — run pip-based fallback (prefer-binary first)
        #[cfg(debug_assertions)]
        println!("uv not found - falling back to pip-based installation (prefer-binary)...");
        append_to_log(&log_path, "uv not found - falling back to pip-based installation (prefer-binary)...");

        let mut install_success = false;

        #[cfg(debug_assertions)]
        println!("Attempting pip install with --prefer-binary to avoid building C extensions...");
        append_to_log(&log_path, "Attempting pip install with --prefer-binary to avoid building C extensions...");

        let mut cmd_prefer = Command::new(&python_path);
        cmd_prefer.args(&["-m", "pip", "install", "--prefer-binary", "-r", &requirements_path.to_string_lossy()]);
        hide_console(&mut cmd_prefer);
        let prefer_binary = cmd_prefer
            .env_remove("PYTHONHOME")
            .env_remove("PYTHONPATH")
            .output();

        match prefer_binary {
            Ok(output) if output.status.success() => {
                #[cfg(debug_assertions)]
                println!("Successfully installed dependencies with --prefer-binary");
                append_to_log(&log_path, "Successfully installed dependencies with --prefer-binary");
                install_success = true;
            }
            Ok(output) => {
                #[cfg(debug_assertions)]
                println!("--prefer-binary install failed, exit code: {:?}", output.status.code());
                // Save stderr for diagnostics
                append_to_log(&log_path, &format!("--prefer-binary failed: {}", String::from_utf8_lossy(&output.stderr)));
                append_to_log(&log_path, &format!("Wrote pip stderr to {:?}", log_path));

                // Try installing faster-whisper directly with prefer-binary
                #[cfg(debug_assertions)]
                println!("Attempting to install faster-whisper directly with --prefer-binary...");
                append_to_log(&log_path, "Attempting to install faster-whisper directly with --prefer-binary...");
                let mut cmd_direct = Command::new(&python_path);
                cmd_direct.args(&["-m", "pip", "install", "--prefer-binary", "faster-whisper"]);
                hide_console(&mut cmd_direct);
                let direct = cmd_direct
                    .env_remove("PYTHONHOME")
                    .env_remove("PYTHONPATH")
                    .output();

                match direct {
                    Ok(out2) if out2.status.success() => {
                        #[cfg(debug_assertions)]
                        println!("Successfully installed faster-whisper directly");
                        append_to_log(&log_path, "Successfully installed faster-whisper directly");
                        install_success = true;
                    }
                    Ok(out2) => {
                        append_to_log(&log_path, &format!("Direct install failed: {}", String::from_utf8_lossy(&out2.stderr)));
                        append_to_log(&log_path, "Direct install also failed");
                    }
                    Err(e) => {
                        #[cfg(debug_assertions)]
                        println!("Failed to execute pip for direct install: {}", e);
                        append_to_log(&log_path, &format!("Failed to execute pip for direct install: {}", e));
                    }
                }
            }
            Err(e) => {
                #[cfg(debug_assertions)]
                println!("Failed to execute pip (prefer-binary): {}", e);
                append_to_log(&log_path, &format!("Failed to execute pip (prefer-binary): {}", e));
            }
        }

        if install_success {
            append_to_log(&log_path, "Installation complete (prefer-binary route)");
            Ok(venv_path)
        } else {
            // Provide actionable guidance in the error message and point to log file
            let log_path = app_data_dir.join("transcription_install.log");
            let guidance = "If you see build errors for 'av' (PyAV) on Windows, try one of the following:\n"
                .to_string()
                + " 1) Install Microsoft Visual C++ Build Tools (Visual Studio C++ workload) and FFmpeg development headers, then retry.\n"
                + " 2) Install a prebuilt PyAV wheel matching your Python version (e.g., from https://www.lfd.uci.edu/~gohlke/pythonlibs/) or use conda: 'conda install -c conda-forge av ffmpeg'.\n"
                + " 3) Run 'pip install --prefer-binary -r requirements.txt' manually to prefer wheels.\n"
                + "Logs from pip were written to: ";

            append_to_log(&log_path, "Installation failed - see logs above");

            Err(format!("Failed to install transcription dependencies. {} Log: {:?}", guidance, log_path))
        }
    }
}

#[tauri::command]
async fn transcribe_audio(audio_path: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    use std::process::Command;
    use tauri::path::BaseDirectory;

    let venv_path = ensure_transcription_dependencies(&app_handle).await?;

    let script_path = app_handle.path().resolve("src/audio/transcription/transcribe.py", BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve transcribe.py resource: {}", e))?;
    
    let python_path = python_executable_in_venv(&venv_path);

    if !python_path.exists() {
        return Err("Python executable not found in venv".to_string());
    }

    // Spawn the transcription script without creating a console window on Windows
    let mut cmd = Command::new(&python_path);
    cmd.arg(&script_path)
        .arg(&audio_path)
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    #[cfg(windows)]
    {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn transcription script: {}", e))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for transcription script: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid output encoding: {}", e))?;

        // Try to parse as JSON
        if let Ok(result) = serde_json::from_str::<serde_json::Value>(&stdout) {
            return serde_json::to_string(&result)
                .map_err(|e| format!("Serialization error: {}", e));
        } else {
            return Err(format!("Invalid JSON output: {}", stdout));
        }
    } else {
        // stderr is inherited so it's already printed, but we can't capture it here for the error message
        // unless we pipe it. But inheriting is better for UX.
        return Err("Transcription script failed (check terminal logs for details)".to_string());
    }
}

#[tauri::command]
async fn install_transcription_dependencies(app_handle: tauri::AppHandle) -> Result<(), String> {
    let _venv_path = ensure_transcription_dependencies(&app_handle).await?;
    Ok(())
}

#[tauri::command]
async fn install_system_dependencies(_app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    // This command no longer executes package managers (apt, brew, winget, etc.).
    // It returns a structured JSON with manual install instructions per platform.
    let instructions: serde_json::Value = if cfg!(windows) {
        serde_json::json!({
            "status": "instructions",
            "message": "Please install the following manually:",
            "packages": [
                {"name": "Python 3.12+", "url": "https://www.python.org/downloads/", "note": "Required for transcription AI"},
                {"name": "FFmpeg", "url": "https://ffmpeg.org/download.html", "note": "Required for audio processing"},
                {"name": "Visual C++ Redistributable", "url": "https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist", "note": "Needed for Python C-extension wheels"},
                {"name": "Git", "url": "https://git-scm.com/download/win", "note": "Required for git sync"}
            ]
        })
    } else if cfg!(target_os = "macos") {
        serde_json::json!({
            "status": "instructions",
            "message": "Run these commands in Terminal:",
            "packages": [
                {"name": "Python 3", "command": "brew install python", "note": "Required for transcription AI"},
                {"name": "FFmpeg", "command": "brew install ffmpeg", "note": "Required for audio processing"},
                {"name": "Git", "command": "brew install git", "note": "Required for git sync"}
            ]
        })
    } else {
        // Linux — provide per-package-manager commands
        serde_json::json!({
            "status": "instructions",
            "message": "Use your distribution's package manager:",
            "packages": [
                {
                    "name": "python3 + pip",
                    "commands": {
                        "apt": "sudo apt install python3 python3-pip",
                        "dnf": "sudo dnf install python3 python3-pip",
                        "pacman": "sudo pacman -S python python-pip"
                    },
                    "note": "Required for transcription AI"
                },
                {
                    "name": "ffmpeg",
                    "commands": {
                        "apt": "sudo apt install ffmpeg",
                        "dnf": "sudo dnf install ffmpeg",
                        "pacman": "sudo pacman -S ffmpeg"
                    },
                    "note": "Required for audio processing"
                },
                {
                    "name": "git",
                    "commands": {
                        "apt": "sudo apt install git",
                        "dnf": "sudo dnf install git",
                        "pacman": "sudo pacman -S git"
                    },
                    "note": "Required for git sync"
                }
            ]
        })
    };
    Ok(instructions)
}

#[tauri::command]
async fn prereflight_check(app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    use std::process::Command;
    use std::time::Duration;
    use tauri::path::BaseDirectory;

    let mut map = serde_json::Map::new();

    // Platform
    map.insert("platform".to_string(), serde_json::Value::String(std::env::consts::OS.to_string()));

    // Check for Python (try py launcher on Windows first, then python)
    let mut python_found = false;
    let mut python_version: Option<String> = None;
    let mut python_exec: Option<String> = None;

    let try_python_cmd = |cmd: &str, args: &[&str]| -> Option<(String, String)> {
        if let Ok(output) = Command::new(cmd).args(args).output() {
            if output.status.success() {
                let out = String::from_utf8_lossy(&output.stdout).to_string();
                let mut lines = out.lines();
                let exe = lines.next().map(|s| s.to_string()).unwrap_or_default();
                let ver = lines.next().map(|s| s.to_string()).unwrap_or_default();
                return Some((exe, ver));
            }
        }
        None
    };

    if cfg!(windows) {
        if let Some((exe, ver)) = try_python_cmd("py", &["-3", "-c", "import sys;print(sys.executable);print(sys.version)"]) {
            python_found = true;
            python_exec = Some(exe);
            python_version = Some(ver);
        }
    }

    if !python_found {
        if let Some((exe, ver)) = try_python_cmd("python", &["-c", "import sys;print(sys.executable);print(sys.version)"]) {
            python_found = true;
            python_exec = Some(exe);
            python_version = Some(ver);
        }
    }

    if !python_found {
        if let Some((exe, ver)) = try_python_cmd("python3", &["-c", "import sys;print(sys.executable);print(sys.version)"]) {
            python_found = true;
            python_exec = Some(exe);
            python_version = Some(ver);
        }
    }

    map.insert("python_found".to_string(), serde_json::Value::Bool(python_found));
    map.insert("python_version".to_string(), match python_version { Some(v) => serde_json::Value::String(v), None => serde_json::Value::Null });
    map.insert("python_executable".to_string(), match python_exec { Some(p) => serde_json::Value::String(p), None => serde_json::Value::Null });

    // Check ffmpeg availability
    let ffmpeg_available = {
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-version");
        hide_console(&mut cmd);
        if let Ok(output) = cmd.output() {
            output.status.success()
        } else {
            false
        }
    };
    map.insert("ffmpeg_available".to_string(), serde_json::Value::Bool(ffmpeg_available));

    // Check for Visual C++ runtime on Windows by probing common DLL locations (vcruntime140.dll)
    // Check for Visual C++ runtime on Windows by probing common DLL locations (vcruntime140.dll)
    if cfg!(windows) {
        let vcruntime_found = std::env::var("WINDIR").ok().map(|w| {
            let sys32 = std::path::Path::new(&w).join("System32").join("vcruntime140.dll");
            let wow64 = std::path::Path::new(&w).join("SysWOW64").join("vcruntime140.dll");
            sys32.exists() || wow64.exists()
        }).unwrap_or(false);
        map.insert("vcruntime_found".to_string(), serde_json::Value::Bool(vcruntime_found));

        // Check that packaged Windows helper exists in resources.
        let windows_bin_path = match app_handle.path().resolve("bin/AudioCapture-x86_64-pc-windows-msvc.exe", BaseDirectory::Resource) {
            Ok(p) if p.exists() => Some(p),
            _ => {
                // Legacy resource path (older setups)
                if let Ok(p) = app_handle.path().resolve("src/audio/windows/Windows.bin", BaseDirectory::Resource) {
                    if p.exists() { Some(p) } else { None }
                } else { None }
            }
        };
        map.insert("windows_helper_present".to_string(), serde_json::Value::Bool(windows_bin_path.as_ref().map(|p| p.exists()).unwrap_or(false)));
        map.insert("windows_helper_path".to_string(), match windows_bin_path { Some(p) => serde_json::Value::String(p.to_string_lossy().to_string()), None => serde_json::Value::Null });
    }

    // MacOS helper check
    if cfg!(target_os = "macos") {
         // ScreenCaptureKit is used natively, so helper functionality is always "present"
         // (assuming OS requirements are met, which is handled by min version checks)
         map.insert("mac_helper_present".to_string(), serde_json::Value::Bool(true));
    }

    // Simple network check to pypi.org (used by pip installs)
    use std::net::ToSocketAddrs;
    let network_ok = {
        let timeout = Duration::from_secs(3);
        match ("pypi.org", 443).to_socket_addrs() {
            Ok(mut addrs) => {
                if let Some(addr) = addrs.next() {
                    std::net::TcpStream::connect_timeout(&addr, timeout).is_ok()
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    };
    map.insert("network_ok".to_string(), serde_json::Value::Bool(network_ok));

    Ok(serde_json::Value::Object(map))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load .env file
    dotenvy::dotenv().ok();

    tauri::Builder::default()
        .manage(git_sync::GitSyncState::new())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_notes_path,
            create_note,
            get_all_notes,
            save_note,
            toggle_star_note,
            delete_note,
            create_folder,
            get_all_folders,
            update_folder,
            delete_folder,
            get_kanban_data,
            save_kanban_data,
            get_trash_items,
            empty_trash,
            restore_from_trash,
            get_google_api_key,
            save_google_api_key,
            remove_google_api_key,
            install_transcription_dependencies,
            install_system_dependencies,
            greet,
            start_recording,
            stop_recording,
            transcribe_audio,
            prereflight_check,
            read_install_log,
            git_sync::git_sync_status,
            git_sync::git_sync_configure,
            git_sync::git_sync_now,
            git_sync::git_sync_force_pull,
            git_sync::git_sync_force_push,
            git_sync::git_sync_disconnect
        ])
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command]
async fn read_install_log(app_handle: tauri::AppHandle) -> Result<String, String> {
    // Return contents of the transcription_install.log in the app data directory (best-effort)
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let log_path = app_data_dir.join("transcription_install.log");
    if log_path.exists() {
        std::fs::read_to_string(&log_path).map_err(|e| format!("Failed to read log file: {}", e))
    } else {
        Ok(String::new())
    }
}
