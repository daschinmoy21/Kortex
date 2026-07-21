use std::fs::File;
use std::io::{Write, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};

// Note: These imports use screencapturekit 1.4.2 API structure.
#[cfg(target_os = "macos")]
use screencapturekit::prelude::*;

// Global state to hold the running stream
#[cfg(target_os = "macos")]
static CAPTURE_STREAM: OnceLock<Mutex<Option<SCStream>>> = OnceLock::new();
static OUTPUT_FILE_PATH: OnceLock<Mutex<Option<String>>> = OnceLock::new();

#[cfg(target_os = "macos")]
struct AudioRecorder {
    file: Arc<Mutex<File>>,
    bytes_written: Arc<Mutex<u64>>,
}

#[cfg(target_os = "macos")]
impl SCStreamOutputTrait for AudioRecorder {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if matches!(of_type, SCStreamOutputType::Audio) {
            // Get the audio buffer list from the sample
            if let Some(audio_buffer_list) = sample.audio_buffer_list() {
                // Iterate over all audio buffers and write their data
                for audio_buffer in audio_buffer_list.iter() {
                    let data = audio_buffer.data();
                    if !data.is_empty() {
                        if let Ok(mut file) = self.file.lock() {
                            if let Ok(_) = file.write_all(data) {
                                if let Ok(mut bytes) = self.bytes_written.lock() {
                                    *bytes += data.len() as u64;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn generate_output_file(app_handle: &AppHandle) -> Result<String, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let file_name = format!("capture_{}", timestamp);

    let audio_dir = app_handle
        .path()
        .app_data_dir()
        .unwrap()
        .join("audio");
    std::fs::create_dir_all(&audio_dir)
        .map_err(|e| format!("Failed to create audio directory: {}", e))?;

    Ok(audio_dir.join(file_name).to_string_lossy().to_string())
}

fn create_wav_header(data_size: u32, sample_rate: u32, channels: u16, bits_per_sample: u16) -> Vec<u8> {
    let byte_rate = sample_rate * (channels as u32) * (bits_per_sample as u32) / 8;
    let block_align = channels * bits_per_sample / 8;
    
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(data_size + 36).to_le_bytes());
    header.extend_from_slice(b"WAVE");
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    header.extend_from_slice(&1u16.to_le_bytes());  // PCM format
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&block_align.to_le_bytes());
    header.extend_from_slice(&bits_per_sample.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_size.to_le_bytes());
    
    header
}

#[cfg(target_os = "macos")]
pub fn start_capture(app_handle: &AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    println!("Starting audio capture (screencapturekit)");
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Starting macOS audio capture with ScreenCaptureKit");

    match start_capture_inner(app_handle) {
        Ok(_) => {
            #[cfg(debug_assertions)]
            eprintln!("[Logia DEBUG] Audio capture started successfully");
            Ok(())
        },
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("[Logia ERROR] Failed to start audio capture: {}", e);
            Err(e)
        }
    }
}

#[cfg(target_os = "macos")]
fn start_capture_inner(app_handle: &AppHandle) -> Result<(), String> {
    // 1. Setup Output File
    let output_base = generate_output_file(app_handle)?;
    let pcm_path = format!("{}.pcm", output_base);
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Output PCM file: {}", pcm_path);
    
    let file = File::create(&pcm_path).map_err(|e| format!("Failed to create PCM file: {}", e))?;
    let file_arc = Arc::new(Mutex::new(file));
    let bytes_written = Arc::new(Mutex::new(0u64));

    // 2. Setup ScreenCaptureKit
    // Create a filter for the main display
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Getting shareable content...");
    let content = SCShareableContent::get().map_err(|e| {
        let err_str = format!("{:?}", e);
        #[cfg(debug_assertions)]
        eprintln!("[Logia ERROR] SCShareableContent::get() failed: {}", err_str);
        if err_str.contains("user declined TCCs") || err_str.contains("NoShareableContent") {
            "PERMISSION_DENIED: Please enable Screen Recording permission for Logia in System Settings > Privacy & Security.".to_string()
        } else {
            format!("Failed to get shareable content: {:?}", e)
        }
    })?;
    
    let displays = content.displays();
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Found {} displays", displays.len());
    
    let display = displays.into_iter().next().ok_or("No display found")?;
    
    // Filter: Include display, exclude nothing
    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();
    
    // Config: Audio capture with minimal video (we only care about audio)
    let config = SCStreamConfiguration::new()
        .with_width(100)  // Minimal video dimensions since we only want audio
        .with_height(100)
        .with_captures_audio(true)
        .with_excludes_current_process_audio(true) // Don't capture our own app's audio
        .with_sample_rate(48000)  // 48kHz sample rate
        .with_channel_count(2);   // Stereo

    // Output Handler
    let recorder = AudioRecorder { 
        file: file_arc,
        bytes_written: bytes_written.clone(),
    };
    
    // Stream - add handler for audio output type
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(recorder, SCStreamOutputType::Audio);
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Starting capture stream...");
    stream.start_capture().map_err(|e| {
        #[cfg(debug_assertions)]
        eprintln!("[Logia ERROR] stream.start_capture() failed: {:?}", e);
        format!("Failed to start capture: {:?}", e)
    })?;
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Capture stream started");

    // Store state
    let state_mutex = CAPTURE_STREAM.get_or_init(|| Mutex::new(None));
    let mut guard = state_mutex.lock().map_err(|e| format!("Mutex error: {}", e))?;
    *guard = Some(stream);

    let path_mutex = OUTPUT_FILE_PATH.get_or_init(|| Mutex::new(None));
    let mut path_guard = path_mutex.lock().map_err(|e| format!("Mutex error: {}", e))?;
    *path_guard = Some(output_base);

    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn start_capture(_app_handle: &AppHandle) -> Result<(), String> {
    Err("Audio capture is only supported on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn stop_capture() -> Result<String, String> {
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Stopping audio capture...");
    
    let state_mutex = CAPTURE_STREAM.get().ok_or("Capture not started")?;
    let mut guard = state_mutex.lock().map_err(|e| format!("Mutex error: {}", e))?;

    if let Some(stream) = guard.take() {
        stream.stop_capture().map_err(|e| {
            #[cfg(debug_assertions)]
            eprintln!("[Logia ERROR] stop_capture() failed: {:?}", e);
            format!("Failed to stop capture: {:?}", e)
        })?;
        #[cfg(debug_assertions)]
        eprintln!("[Logia DEBUG] Capture stream stopped");
    } else {
        return Err("Capture not running".to_string());
    }

    // Convert PCM to WAV
    let path_mutex = OUTPUT_FILE_PATH.get().ok_or("Output path lost")?;
    let mut path_guard = path_mutex.lock().map_err(|e| format!("Mutex error: {}", e))?;
    let output_base = path_guard.take().ok_or("Output path empty")?;

    let pcm_path = format!("{}.pcm", output_base);
    let wav_path = format!("{}.wav", output_base);
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Converting PCM to WAV: {} -> {}", pcm_path, wav_path);

    // Read PCM data
    let mut pcm_file = File::open(&pcm_path).map_err(|e| format!("Failed to open PCM: {}", e))?;
    let mut pcm_data = Vec::new();
    pcm_file.read_to_end(&mut pcm_data).map_err(|e| format!("Failed to read PCM: {}", e))?;
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Read {} bytes of PCM data", pcm_data.len());

    if pcm_data.is_empty() {
        // Clean up and return error
        let _ = std::fs::remove_file(&pcm_path);
        return Err("No audio data captured. Make sure system audio is playing.".to_string());
    }

    // System audio from ScreenCaptureKit is 32-bit float, need to convert to 16-bit PCM for WAV
    // However, the format depends on the ScreenCaptureKit configuration
    // The raw bytes are interleaved stereo 32-bit float at 48kHz
    
    // Convert Float32 to Int16 for standard WAV compatibility
    let sample_count = pcm_data.len() / 4; // 4 bytes per float32 sample
    let mut int16_data = Vec::with_capacity(sample_count * 2);
    
    for chunk in pcm_data.chunks_exact(4) {
        let float_sample = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // Clamp and convert to i16 range
        let clamped = float_sample.clamp(-1.0, 1.0);
        let int16_sample = (clamped * 32767.0) as i16;
        int16_data.extend_from_slice(&int16_sample.to_le_bytes());
    }
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] Converted to {} bytes of 16-bit PCM", int16_data.len());

    // Create WAV file with appropriate header
    // 48000 Hz, Stereo, 16-bit
    let header = create_wav_header(int16_data.len() as u32, 48000, 2, 16);
    let mut wav_file = File::create(&wav_path).map_err(|e| format!("Failed to create WAV: {}", e))?;
    
    wav_file.write_all(&header).map_err(|e| format!("Write header failed: {}", e))?;
    wav_file.write_all(&int16_data).map_err(|e| format!("Write data failed: {}", e))?;

    // Clean up PCM file
    let _ = std::fs::remove_file(&pcm_path);
    
    #[cfg(debug_assertions)]
    eprintln!("[Logia DEBUG] WAV file created successfully: {}", wav_path);

    Ok(wav_path)
}

#[cfg(not(target_os = "macos"))]
pub fn stop_capture() -> Result<String, String> {
    Err("Not supported".to_string())
}
