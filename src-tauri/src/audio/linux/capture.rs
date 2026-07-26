use libc;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
static CAPTURE_PROCESS: OnceLock<Mutex<Option<std::process::Child>>> = OnceLock::new();
static CURRENT_FILE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn generate_output_file(app_handle: &AppHandle) -> Result<String, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let file_name = format!("capture_{}.wav", timestamp);

    let audio_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?
        .join("audio");
    std::fs::create_dir_all(&audio_dir)
        .map_err(|e| format!("Failed to create audio directory: {}", e))?;

    Ok(audio_dir.join(file_name).to_string_lossy().to_string())
}

fn get_default_sink() -> Result<String, String> {
    let output = Command::new("pactl")
        .args(&["get-default-sink"])
        .output()
        .map_err(|e| format!("Failed to execute pactl{}", e))?;

    if !output.status.success() {
        return Err(format!(
            "Failed to get default sink:{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let sink_name = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in sink name :{}", e))?
        .trim()
        .to_string();

    if sink_name.is_empty() {
        return Err("No default sink found".to_string());
    }
    Ok(sink_name)
}

pub fn start_capture(app_handle: &AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    println!("Starting audio capture on Linux");
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Starting audio capture on Linux");
    
    let default_sink = get_default_sink().map_err(|e| {
        #[cfg(debug_assertions)]
        eprintln!("[Logia ERROR] Failed to get default sink: {}", e);
        e
    })?;
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Got default sink: {}", default_sink);
    
    let monitor_name = format!("{}.monitor", default_sink);
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Monitor name: {}", monitor_name);
    
    let output_file = generate_output_file(app_handle)?;
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Output file: {}", output_file);

    let mut cmd = Command::new("ffmpeg");
    cmd.args(&[
        "-f",
        "pulse", // PulseAudio input format
        "-i",
        &monitor_name, // Input from default sink's monitor
        "-acodec",
        "pcm_s16le", // 16-bit little-endian PCM
        "-ar",
        "16000", // Sample rate (good for speech)
        "-ac",
        "1",          // Mono audio
        "-y",         // Overwrite output file if exists
        &output_file, // Output filename
    ]);

    let child = cmd
        .spawn()
        .map_err(|e| {
            #[cfg(debug_assertions)]
            eprintln!("[Logia ERROR] Failed to start ffmpeg: {}", e);
            format!("Failed to start FFMPEG:{}", e)
        })?;
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] ffmpeg spawned with PID: {}", child.id());

    let process_mutex = CAPTURE_PROCESS.get_or_init(|| Mutex::new(None));
    let mut guard = process_mutex
        .lock()
        .map_err(|e| format!("Mutex error:{}", e))?;
    *guard = Some(child);

    let file_mutex = CURRENT_FILE.get_or_init(|| Mutex::new(None));
    let mut file_guard = file_mutex
        .lock()
        .map_err(|e| format!("Mutex error:{}", e))?;
    *file_guard = Some(output_file.clone());

    #[cfg(debug_assertions)]
    println!("Capturing audio from '{}' to {}", monitor_name, output_file);
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Capture started successfully");
    Ok(())
}

pub fn stop_capture() -> Result<String, String> {
    let process_mutex = CAPTURE_PROCESS
        .get()
        .ok_or("Capture process not initialized")?;

    let mut guard = process_mutex
        .lock()
        .map_err(|e| format!("Mutex error {}", e))?;
    if let Some(mut child) = guard.take() {
        unsafe {
            libc::kill(child.id() as i32, libc::SIGINT);
        }

        std::thread::sleep(std::time::Duration::from_millis(500));

        if let Ok(None) = child.try_wait() {
            if let Err(e) = child.kill() {
                return Err(format!("Failed to kill process:{}", e));
            }
        }

        child
            .wait()
            .map_err(|e| format!("Process wait failed:{}", e))?;
        #[cfg(debug_assertions)]
        println!("Audio capture stopped");
    }

    let file_mutex = CURRENT_FILE
        .get()
        .ok_or("File not initialized")?;
    let mut file_guard = file_mutex
        .lock()
        .map_err(|e| format!("Mutex error:{}", e))?;
    let file_path = file_guard.take().ok_or("No file path stored")?;

    Ok(file_path)
}

pub fn cleanup() -> Result<(), String> {
    stop_capture();
    #[cfg(debug_assertions)]
    println!("Cleanup done");
    Ok(())
}
