use chrono;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::Manager;
use uuid::Uuid;
use keyring::Entry;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;
use fs2::FileExt;

mod audio;
mod git_sync;

/// Atomically write content to a file using a temporary file and rename.
/// This prevents data loss during concurrent writes and is safe across filesystem operations.
fn atomic_write_file(path: &std::path::Path, content: &str) -> Result<(), String> {
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

/// Validate that a user-supplied id is safe to use as a filename segment.
/// Rejects path separators, parent traversal, and other unexpected characters.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn require_safe_id(id: &str) -> Result<(), String> {
    if is_safe_id(id) {
        Ok(())
    } else {
        Err(format!("Invalid id: contains disallowed characters or path components"))
    }
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

// Keyring service for the encryption master key
const KEYRING_SERVICE_MASTER: &str = "Logia";
const KEYRING_USERNAME_MASTER: &str = "encryption_master_key";

/// Get or create the per-installation encryption key.
/// The key is stored in the OS keyring for security. If keyring is unavailable,
/// we generate a key and store it in a local file (less secure, but functional).
fn get_or_create_encryption_key(app_handle: &tauri::AppHandle) -> Result<[u8; 32], String> {
    // Try to get from keyring first
    if let Ok(entry) = Entry::new(KEYRING_SERVICE_MASTER, KEYRING_USERNAME_MASTER) {
        if let Ok(key_b64) = entry.get_password() {
            if let Ok(key_bytes) = general_purpose::STANDARD.decode(&key_b64) {
                if key_bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&key_bytes);
                    return Ok(key);
                }
            }
        }
    }

    // Key not in keyring - check file fallback
    let config_dir = get_config_directory(app_handle)?;
    let key_file = config_dir.join(".encryption_key");
    
    if key_file.exists() {
        if let Ok(content) = fs::read_to_string(&key_file) {
            if let Ok(key_bytes) = general_purpose::STANDARD.decode(content.trim()) {
                if key_bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&key_bytes);
                    // Try to migrate to keyring
                    let key_b64 = general_purpose::STANDARD.encode(&key);
                    if let Ok(entry) = Entry::new(KEYRING_SERVICE_MASTER, KEYRING_USERNAME_MASTER) {
                        let _ = entry.set_password(&key_b64);
                    }
                    return Ok(key);
                }
            }
        }
    }

    // Generate new key
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    let key_b64 = general_purpose::STANDARD.encode(&key);
    
    // Try to store in keyring
    if let Ok(entry) = Entry::new(KEYRING_SERVICE_MASTER, KEYRING_USERNAME_MASTER) {
        let _ = entry.set_password(&key_b64);
    }
    
    // Fallback: store in file
    if let Err(e) = fs::write(&key_file, &key_b64) {
        return Err(format!("Failed to store encryption key: {}", e));
    }
    
    Ok(key)
}

fn encrypt_api_key(app_handle: &tauri::AppHandle, plaintext: &str) -> Result<String, String> {
    let key_bytes = get_or_create_encryption_key(app_handle)?;
    let cipher_key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(cipher_key);
    
    // Generate random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext (nonce doesn't need to be secret)
    let mut combined = nonce_bytes.to_vec();
    combined.extend(ciphertext);
    
    Ok(general_purpose::STANDARD.encode(combined))
}

fn decrypt_api_key(app_handle: &tauri::AppHandle, encrypted: &str) -> Result<String, String> {
    let key_bytes = get_or_create_encryption_key(app_handle)?;
    let cipher_key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(cipher_key);

    let combined = general_purpose::STANDARD.decode(encrypted)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    
    if combined.len() < 12 {
        return Err("Invalid encrypted data: too short".to_string());
    }
    
    // Extract nonce (first 12 bytes) and ciphertext (rest)
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext)
        .map_err(|e| format!("UTF-8 decode failed: {}", e))
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Folder {
    id: String,
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KanbanTask {
    id: String,
    name: String,
    column: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Note {
    id: String,
    title: String,
    content: String,
    created_at: String,
    updated_at: String,
    #[serde(default = "default_note_type")]
    note_type: String,
    #[serde(default)]
    folder_id: Option<String>,
    #[serde(default)]
    starred: bool,
}

fn default_note_type() -> String {
    "text".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrashItem {
    pub id: String,
    pub title: String,  // Title from note or name from folder
    pub original_type: String,  // "note" or "folder"
    pub filename: String,
    pub deleted_at: String,
}

fn resolve_logia_dir(app_handle: &tauri::AppHandle, subdir: &str) -> Result<PathBuf, String> {
    // Try standard XDG documents directory first
    let dir_result = app_handle
        .path()
        .resolve(format!("Logia/{}", subdir), BaseDirectory::Document);

    let target_dir = match dir_result {
        Ok(path) => path,
        Err(_) => {
            // Fallback: Try manual construction relative to home
            let home_dir = app_handle
                .path()
                .resolve("", BaseDirectory::Home)
                .map_err(|_| "Could not resolve home directory")?;
            
            home_dir.join("Documents").join("Logia").join(subdir)
        }
    };

    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create {} directory at {:?}: {}", subdir, target_dir, e))?;
    }

    Ok(target_dir)
}

fn get_notes_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "notes")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

fn get_folders_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "folders")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

fn get_config_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app_handle
        .path()
        .resolve("Logia", BaseDirectory::AppConfig)
        .map_err(|_| "Could not find config directory")?;

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config directory:{}", e))?;
    }

    Ok(config_dir)
}

fn get_kanban_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "kanban")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

fn get_trash_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "trash")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

#[tauri::command]
async fn get_notes_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let notes_dir = get_notes_directory(&app_handle)?;
    Ok(notes_dir.to_string_lossy().to_string())
}

#[tauri::command]
async fn create_note(
    title: String,
    note_type: String,
    folder_id: Option<String>,
    content: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<Note, String> {
    let notes_dir = get_notes_directory(&app_handle)?;
    let note_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let note = Note {
        id: note_id.clone(),
        title: title.clone(),
        content: content.unwrap_or_default(),
        created_at: now.clone(),
        updated_at: now,
        note_type,
        folder_id,
        starred: false,
    };

    let file_path = notes_dir.join(format!("{}.json", note_id));
    let note_json = serde_json::to_string_pretty(&note)
        .map_err(|e| format!("Failed to serialize note: {}", e))?;

    atomic_write_file(&file_path, &note_json)?;

    Ok(note)
}

#[tauri::command]
async fn get_all_notes(app_handle: tauri::AppHandle) -> Result<Vec<Note>, String> {
    let notes_dir = get_notes_directory(&app_handle)?;
    let mut notes = Vec::new();

    if let Ok(entries) = fs::read_dir(&notes_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(note) = serde_json::from_str::<Note>(&content) {
                            notes.push(note);
                        }
                    }
                }
            }
        }
    }

    // Sort by updated_at descending
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

#[tauri::command]
async fn save_note(note: Note, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&note.id)?;
    let notes_dir = get_notes_directory(&app_handle)?;
    let file_path = notes_dir.join(format!("{}.json", note.id));

    let mut updated_note = note;
    updated_note.updated_at = chrono::Utc::now().to_rfc3339();

    let note_json = serde_json::to_string_pretty(&updated_note)
        .map_err(|e| format!("Failed to serialize note: {}", e))?;

    atomic_write_file(&file_path, &note_json)?;

    Ok(())
}

#[tauri::command]
async fn toggle_star_note(note_id: String, app_handle: tauri::AppHandle) -> Result<bool, String> {
    require_safe_id(&note_id)?;
    let notes_dir = get_notes_directory(&app_handle)?;
    let file_path = notes_dir.join(format!("{}.json", note_id));
    
    // Read the note
    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read note: {}", e))?;
    let mut note: Note = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse note: {}", e))?;
    
    // Toggle starred status
    note.starred = !note.starred;
    note.updated_at = chrono::Utc::now().to_rfc3339();
    
    // Save the note
    let note_json = serde_json::to_string_pretty(&note)
        .map_err(|e| format!("Failed to serialize note: {}", e))?;
    atomic_write_file(&file_path, &note_json)?;
    
    Ok(note.starred)
}

#[tauri::command]
async fn delete_note(note_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&note_id)?;
    let notes_dir = get_notes_directory(&app_handle)?;
    let trash_dir = get_trash_directory(&app_handle)?;
    let source_path = notes_dir.join(format!("{}.json", note_id));
    let dest_path = trash_dir.join(format!("note_{}.json", note_id));
    
    // Move to trash instead of deleting
    // Use copy + remove as fallback if rename fails (cross-filesystem)
    if let Err(_) = fs::rename(&source_path, &dest_path) {
        fs::copy(&source_path, &dest_path)
            .map_err(|e| format!("Failed to copy note to trash: {}", e))?;
        fs::remove_file(&source_path)
            .map_err(|e| format!("Failed to remove original note: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
async fn create_folder(
    name: String,
    parent_id: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<Folder, String> {
    let folders_dir = get_folders_directory(&app_handle)?;
    let folder_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let folder = Folder {
        id: folder_id.clone(),
        name,
        parent_id,
        created_at: now.clone(),
        updated_at: now,
    };

    let file_path = folders_dir.join(format!("{}.json", folder_id));
    let folder_json = serde_json::to_string_pretty(&folder)
        .map_err(|e| format!("Failed to serialize folder: {}", e))?;

    atomic_write_file(&file_path, &folder_json)?;

    Ok(folder)
}

#[tauri::command]
async fn get_all_folders(app_handle: tauri::AppHandle) -> Result<Vec<Folder>, String> {
    let folders_dir = get_folders_directory(&app_handle)?;
    let mut folders = Vec::new();

    if let Ok(entries) = fs::read_dir(&folders_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(folder) = serde_json::from_str::<Folder>(&content) {
                            folders.push(folder);
                        }
                    }
                }
            }
        }
    }

    // Sort by created_at
    folders.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(folders)
}

#[tauri::command]
async fn update_folder(folder: Folder, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&folder.id)?;
    let folders_dir = get_folders_directory(&app_handle)?;
    let file_path = folders_dir.join(format!("{}.json", folder.id));

    let mut updated_folder = folder;
    updated_folder.updated_at = chrono::Utc::now().to_rfc3339();

    let folder_json = serde_json::to_string_pretty(&updated_folder)
        .map_err(|e| format!("Failed to serialize folder: {}", e))?;

    atomic_write_file(&file_path, &folder_json)?;

    Ok(())
}

#[tauri::command]
async fn delete_folder(folder_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&folder_id)?;
    let folders_dir = get_folders_directory(&app_handle)?;
    let notes_dir = get_notes_directory(&app_handle)?;
    let trash_dir = get_trash_directory(&app_handle)?;
    let source_path = folders_dir.join(format!("{}.json", folder_id));
    let dest_path = trash_dir.join(format!("folder_{}.json", folder_id));

    // Also trash notes that belong to this folder so they aren't orphaned on disk
    // while disappearing from the UI (frontend filters them out of state).
    if let Ok(entries) = fs::read_dir(&notes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(note) = serde_json::from_str::<Note>(&content) {
                    if note.folder_id.as_deref() == Some(folder_id.as_str()) {
                        let note_trash = trash_dir.join(format!("note_{}.json", note.id));
                        if fs::rename(&path, &note_trash).is_err() {
                            let _ = fs::copy(&path, &note_trash);
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }
    
    // Move folder metadata to trash instead of deleting
    if let Err(_) = fs::rename(&source_path, &dest_path) {
        fs::copy(&source_path, &dest_path)
            .map_err(|e| format!("Failed to copy folder to trash: {}", e))?;
        fs::remove_file(&source_path)
            .map_err(|e| format!("Failed to remove original folder: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
async fn get_kanban_data(app_handle: tauri::AppHandle) -> Result<Vec<KanbanTask>, String> {
    let kanban_dir = get_kanban_directory(&app_handle)?;
    let file_path = kanban_dir.join("data.json");

    if file_path.exists() {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read kanban data: {}", e))?;
        let tasks: Vec<KanbanTask> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse kanban data: {}", e))?;
        Ok(tasks)
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
async fn save_kanban_data(tasks: Vec<KanbanTask>, app_handle: tauri::AppHandle) -> Result<(), String> {
    let kanban_dir = get_kanban_directory(&app_handle)?;
    let file_path = kanban_dir.join("data.json");

    let data_json = serde_json::to_string_pretty(&tasks)
        .map_err(|e| format!("Failed to serialize kanban data: {}", e))?;

    atomic_write_file(&file_path, &data_json)?;

    Ok(())
}

// --- Trash Management Commands ---

#[tauri::command]
async fn get_trash_items(app_handle: tauri::AppHandle) -> Result<Vec<TrashItem>, String> {
    let trash_dir = get_trash_directory(&app_handle)?;
    let mut items = Vec::new();

    if let Ok(entries) = fs::read_dir(&trash_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let filename = path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                
                // Parse the prefix to determine type
                let (original_type, id) = if filename.starts_with("note_") {
                    ("note".to_string(), filename.trim_start_matches("note_").trim_end_matches(".json").to_string())
                } else if filename.starts_with("folder_") {
                    ("folder".to_string(), filename.trim_start_matches("folder_").trim_end_matches(".json").to_string())
                } else {
                    continue; // Unknown format, skip
                };

                // Get file modification time as deleted_at
                let deleted_at = if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339()
                    } else {
                        chrono::Utc::now().to_rfc3339()
                    }
                } else {
                    chrono::Utc::now().to_rfc3339()
                };

                // Read the JSON content to extract title/name
                let title = if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                        // For notes, get "title"; for folders, get "name"
                        json.get("title")
                            .or_else(|| json.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Untitled")
                            .to_string()
                    } else {
                        "Untitled".to_string()
                    }
                } else {
                    "Untitled".to_string()
                };

                items.push(TrashItem {
                    id,
                    title,
                    original_type,
                    filename,
                    deleted_at,
                });
            }
        }
    }

    // Sort by deleted_at descending (newest first)
    items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    Ok(items)
}

#[tauri::command]
async fn empty_trash(app_handle: tauri::AppHandle) -> Result<usize, String> {
    let trash_dir = get_trash_directory(&app_handle)?;
    let mut count = 0;

    if let Ok(entries) = fs::read_dir(&trash_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if fs::remove_file(&path).is_ok() {
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

#[tauri::command]
async fn restore_from_trash(item_id: String, item_type: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&item_id)?;
    let trash_dir = get_trash_directory(&app_handle)?;
    
    let (trash_filename, dest_dir) = match item_type.as_str() {
        "note" => (
            format!("note_{}.json", item_id),
            get_notes_directory(&app_handle)?
        ),
        "folder" => (
            format!("folder_{}.json", item_id),
            get_folders_directory(&app_handle)?
        ),
        _ => return Err("Invalid item type".to_string()),
    };

    let source_path = trash_dir.join(&trash_filename);
    let dest_path = dest_dir.join(format!("{}.json", item_id));

    if !source_path.exists() {
        return Err("Item not found in trash".to_string());
    }

    // Refuse to overwrite an existing live item (prevents silent data loss)
    if dest_path.exists() {
        return Err(format!(
            "Cannot restore: a {} with this id already exists. Rename or delete the existing item first.",
            item_type
        ));
    }

    // Ensure destination directory exists
    if !dest_dir.exists() {
        fs::create_dir_all(&dest_dir)
            .map_err(|e| format!("Failed to create destination directory: {}", e))?;
    }

    // Move back from trash
    if let Err(_) = fs::rename(&source_path, &dest_path) {
        fs::copy(&source_path, &dest_path)
            .map_err(|e| format!("Failed to restore item: {}", e))?;
        fs::remove_file(&source_path)
            .map_err(|e| format!("Failed to remove from trash: {}", e))?;
    }

    Ok(())
}

// Keyring helpers: try keyring first, fallback to encrypted config file when unavailable
fn try_get_keyring(service: &str, username: &str) -> Option<String> {
    if let Ok(entry) = Entry::new(service, username) {
        if let Ok(pw) = entry.get_password() {
            return Some(pw);
        }
    }
    None
}

fn try_set_keyring(service: &str, username: &str, secret: &str) -> bool {
    if let Ok(entry) = Entry::new(service, username) {
        return entry.set_password(secret).is_ok();
    }
    false
}

fn try_delete_keyring(service: &str, username: &str) -> bool {
    if let Ok(entry) = Entry::new(service, username) {
        // older/newer API differences: try both methods if available
        let _ = entry.delete_credential();
        // delete_credential returns Result<(), _> in some versions; ignore errors
        return true;
    }
    false
}

// Service/username used for storing the Google API key
const KEYRING_SERVICE: &str = "Logia";
const KEYRING_USERNAME: &str = "google_api_key";

#[tauri::command]
async fn get_google_api_key(app_handle: tauri::AppHandle) -> Result<String, String> {
    // Try keyring first (works on Windows Credential Manager, macOS Keychain, Linux Secret Service)
    if let Some(pw) = try_get_keyring(KEYRING_SERVICE, KEYRING_USERNAME) {
        return Ok(pw);
    }

    // Fallback: check config.json for encrypted key
    let config_dir = get_config_directory(&app_handle)?;
    let config_file = config_dir.join("config.json");

    if config_file.exists() {
        let content = fs::read_to_string(&config_file)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        let config: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;

        // Check for encrypted key
        if let Some(encrypted_key) = config.get("encrypted_google_api_key").and_then(|v| v.as_str()) {
            let key = decrypt_api_key(&app_handle, encrypted_key)?;
            // Try to migrate into keyring for future
            let _ = try_set_keyring(KEYRING_SERVICE, KEYRING_USERNAME, &key);
            return Ok(key);
        }

        // Legacy: Check for plain key and migrate
        if let Some(plain_key) = config.get("google_api_key").and_then(|v| v.as_str()) {
            // Migrate to keyring if possible
            if try_set_keyring(KEYRING_SERVICE, KEYRING_USERNAME, plain_key) {
                // Remove plain key from config
                let mut updated_config = config.clone();
                if let Some(obj) = updated_config.as_object_mut() {
                    obj.remove("google_api_key");
                    // also attempt to store encrypted form
                    if let Ok(encrypted) = encrypt_api_key(&app_handle, plain_key) {
                        obj.insert("encrypted_google_api_key".to_string(), serde_json::Value::String(encrypted));
                    }
                }
                let content = serde_json::to_string_pretty(&updated_config).unwrap_or_default();
                let _ = atomic_write_file(&config_file, &content);
            }

            return Ok(plain_key.to_string());
        }
    }

    Err("API key not configured".to_string())
}

#[tauri::command]
async fn save_google_api_key(key: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    // First attempt to save to keyring (preferred)
    if try_set_keyring(KEYRING_SERVICE, KEYRING_USERNAME, &key) {
        // Also persist an encrypted copy to config.json as a fallback for dev/reload scenarios
        let encrypted_key = encrypt_api_key(&app_handle, &key)?;
        let config_dir = get_config_directory(&app_handle)?;
        let config_file = config_dir.join("config.json");

        let mut config = if config_file.exists() {
            if let Ok(content) = fs::read_to_string(&config_file) {
                serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({})
            }
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = config.as_object_mut() {
            obj.insert("encrypted_google_api_key".to_string(), serde_json::Value::String(encrypted_key));
            // Remove any plain key
            obj.remove("google_api_key");
        }

        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let _ = atomic_write_file(&config_file, &content);

        // Also remove plain key from config.json if present
        // (already removed above)
        return Ok(());
    }

    // Fallback to encrypted config.json
    let encrypted_key = encrypt_api_key(&app_handle, &key)?;

    let config_dir = get_config_directory(&app_handle)?;
    let config_file = config_dir.join("config.json");

    let mut config = if config_file.exists() {
        if let Ok(content) = fs::read_to_string(&config_file) {
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        }
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = config.as_object_mut() {
        obj.insert("encrypted_google_api_key".to_string(), serde_json::Value::String(encrypted_key));
        // Remove any plain key
        obj.remove("google_api_key");
    }

    let content = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    atomic_write_file(&config_file, &content)?;

    Ok(())
}

#[tauri::command]
async fn remove_google_api_key(app_handle: tauri::AppHandle) -> Result<(), String> {
    // Try to remove from keyring
    let _ = try_delete_keyring(KEYRING_SERVICE, KEYRING_USERNAME);

    // Also remove from config.json
    let config_dir = get_config_directory(&app_handle)?;
    let config_file = config_dir.join("config.json");

    if config_file.exists() {
        let content = fs::read_to_string(&config_file)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        let mut config: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;

        if let Some(obj) = config.as_object_mut() {
            obj.remove("google_api_key");
            obj.remove("encrypted_google_api_key");
        }

        let content = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        atomic_write_file(&config_file, &content)?;
    }

    Ok(())
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
                println!("faster_whisper already available in system Python");
                append_to_log(&log_path, "faster_whisper already available in system Python");
                return Ok(app_data_dir.join("system_python_override"));
            }
        }
        
        // faster_whisper not installed - try to install it using uv to a local venv
        println!("faster_whisper not found in system Python, installing to local venv...");
        append_to_log(&log_path, "faster_whisper not found, creating local venv for Nix...");
        
        let nix_venv_path = app_data_dir.join("transcription_venv");
        
        // Create venv using uv with system Python
        let mut cmd_uv_venv = Command::new("uv");
        cmd_uv_venv.args(&["venv", &nix_venv_path.to_string_lossy(), "--python", &system_python_path, "--clear"]);
        hide_console(&mut cmd_uv_venv);
        
        if let Ok(status) = cmd_uv_venv.status() {
            if status.success() {
                println!("Created venv for Nix using uv");
                append_to_log(&log_path, "Created venv for Nix using uv");
                
                // Install faster-whisper
                let mut cmd_install = Command::new("uv");
                cmd_install.args(&["pip", "install", "faster-whisper", "--python", &nix_venv_path.to_string_lossy()]);
                hide_console(&mut cmd_install);
                
                if let Ok(install_status) = cmd_install.status() {
                    if install_status.success() {
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
                    println!("Detected Python 3.14 in venv, which is likely incompatible. Recreating venv with 3.12...");
                    append_to_log(&log_path, "Detected incompatible Python in venv, removing venv to recreate");
                    let _ = std::fs::remove_dir_all(&venv_path);
                }
            }
        } else {
            // Venv dir exists but python binary is missing? it's broken.
            println!("Venv directory exists but python binary missing. Recreating...");
            append_to_log(&log_path, "Venv broken (no python binary), removing to recreate");
            let _ = std::fs::remove_dir_all(&venv_path);
        }
    }

    // Create virtual environment if it doesn't exist
    if !venv_path.exists() {
        println!("Creating virtual environment...");
        append_to_log(&log_path, "Creating virtual environment...");

        let mut venv_created = if uv_available {
            let mut cmd_uv_venv = Command::new("uv");
            cmd_uv_venv.args(&["venv", &venv_path.to_string_lossy(), "--python", "3.12"]);
            hide_console(&mut cmd_uv_venv);
            let status = cmd_uv_venv.status().map(|s| s.success()).unwrap_or(false);
            if status {
                println!("Created venv with uv (Python 3.12)");
                append_to_log(&log_path, "Created venv with uv (Python 3.12)");
            } else {
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
                println!("Created venv with py launcher");
                append_to_log(&log_path, "Created venv with py launcher");
            }

            if !created_with_py {
                let mut cmd_python = Command::new("python");
                cmd_python.args(&["-m", "venv", &path_str]);
                hide_console(&mut cmd_python);
                let created_with_python = cmd_python.status().map(|s| s.success()).unwrap_or(false);

                if created_with_python {
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
            println!("uv failed or not available, falling back to python3...");
            append_to_log(&log_path, "uv failed/missing, falling back to python3");

            // Unix-like fallback to python3
            let mut cmd_py3 = Command::new("python3");
            cmd_py3.args(&["-m", "venv", &venv_path.to_string_lossy()]);
            hide_console(&mut cmd_py3);
            let status = cmd_py3.status().map(|s| s.success()).unwrap_or(false);

            if status {
                println!("Created venv with python3");
                append_to_log(&log_path, "Created venv with python3");
            }
            venv_created = status;
        }

        if !venv_created {
            append_to_log(&log_path, "Failed to create virtual environment");
            return Err("Failed to create virtual environment".to_string());
        } else{
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
    println!("Upgrading pip/setuptools/wheel and installing build helpers (cython, imageio-ffmpeg)...");
    append_to_log(&log_path, "Upgrading pip/setuptools/wheel and installing build helpers (cython, imageio-ffmpeg)...");
    let mut cmd_upgrade = Command::new(&python_path);
    cmd_upgrade.args(&["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel", "cython", "imageio-ffmpeg"]);
    hide_console(&mut cmd_upgrade);
    let _ = cmd_upgrade
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .status()
        .map(|s| if s.success() { println!("Build helpers installed/updated"); } else { println!("Warning: failed to upgrade/install build helpers (exit code: {:?})", s.code()); });
    
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
            println!("faster_whisper already installed in venv.");
            append_to_log(&log_path, "faster_whisper already installed in venv.");
            return Ok(venv_path); // Dependencies already installed, return venv_path
        }
    }

    // Install dependencies if not already installed
    println!("Installing transcription dependencies...");
    append_to_log(&log_path, "Installing transcription dependencies...");

    // Prefer uv for installs. If uv is available, use it exclusively. If uv is not present, fall back to pip routes.
    if uv_available {
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
                println!("Successfully installed dependencies with uv");
                append_to_log(&log_path, "Successfully installed dependencies with uv");
                return Ok(venv_path);
            },
            Ok(status) => {
                println!("uv install failed with exit code: {:?}", status.code());
                append_to_log(&log_path, &format!("uv failed with exit code: {:?}", status.code()));
                // Do not fall back automatically when uv exists; surface the error and advise user
                return Err(format!("uv failed to install dependencies (exit code {:?}). Try running 'uv pip install -r {}'.", status.code(), requirements_path.to_string_lossy()));
            },
            Err(e) => {
                println!("Failed to execute uv: {}", e);
                append_to_log(&log_path, &format!("Failed to execute uv: {}", e));
                return Err(format!("Failed to execute uv: {}", e));
            }
        }
    } else {
        // uv not available — run pip-based fallback (prefer-binary first)
        println!("uv not found - falling back to pip-based installation (prefer-binary)...");
        append_to_log(&log_path, "uv not found - falling back to pip-based installation (prefer-binary)...");

        let mut install_success = false;

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
                println!("Successfully installed dependencies with --prefer-binary");
                append_to_log(&log_path, "Successfully installed dependencies with --prefer-binary");
                install_success = true;
            }
            Ok(output) => {
                println!("--prefer-binary install failed, exit code: {:?}", output.status.code());
                // Save stderr for diagnostics
                append_to_log(&log_path, &format!("--prefer-binary failed: {}", String::from_utf8_lossy(&output.stderr)));
                append_to_log(&log_path, &format!("Wrote pip stderr to {:?}", log_path));

                // Try installing faster-whisper directly with prefer-binary
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
                        println!("Successfully installed faster-whisper directly");
                        append_to_log(&log_path, "Successfully installed faster-whisper directly");
                        install_success = true;
                    }
                    Ok(out2) => {
                        append_to_log(&log_path, &format!("Direct install failed: {}", String::from_utf8_lossy(&out2.stderr)));
                        append_to_log(&log_path, "Direct install also failed");
                    }
                    Err(e) => {
                        println!("Failed to execute pip for direct install: {}", e);
                        append_to_log(&log_path, &format!("Failed to execute pip for direct install: {}", e));
                    }
                }
            }
            Err(e) => {
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
async fn install_system_dependencies(app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    use std::process::Command;

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;
    let log_path = app_data_dir.join("transcription_install.log");
    fn append_to_log(path: &std::path::PathBuf, msg: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(msg.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }

    append_to_log(&log_path, "Starting system dependency installer (best-effort)");

    let mut results = serde_json::Map::new();

    if cfg!(windows) {
        // Try winget first
        append_to_log(&log_path, "Windows detected: trying winget to install Python and ffmpeg");
        let mut cmd_w_py = Command::new("winget");
        cmd_w_py.args(&["install", "--id", "Python.Python.3", "-e", "--silent"]);
        hide_console(&mut cmd_w_py);
        let winget_py = cmd_w_py.status();
        append_to_log(&log_path, &format!("winget python status: {:?}", winget_py));
        let mut cmd_w_ff = Command::new("winget");
        cmd_w_ff.args(&["install", "--id", "Gyan.FFmpeg", "-e", "--silent"]);
        hide_console(&mut cmd_w_ff);
        let winget_ff = cmd_w_ff.status();
        append_to_log(&log_path, &format!("winget ffmpeg status: {:?}", winget_ff));

        // Fallback to choco if winget not present
        let mut cmd_ch_py = Command::new("choco");
        cmd_ch_py.args(&["install", "python", "-y"]);
        hide_console(&mut cmd_ch_py);
        let choco_py = cmd_ch_py.status();
        append_to_log(&log_path, &format!("choco python status: {:?}", choco_py));
        let mut cmd_ch_ff = Command::new("choco");
        cmd_ch_ff.args(&["install", "ffmpeg", "-y"]);
        hide_console(&mut cmd_ch_ff);
        let choco_ff = cmd_ch_ff.status();
        append_to_log(&log_path, &format!("choco ffmpeg status: {:?}", choco_ff));

        // Installer cannot reliably install Visual C++ redistributable automatically; link user instead
        results.insert("python_attempted".to_string(), serde_json::Value::Bool(true));
        results.insert("ffmpeg_attempted".to_string(), serde_json::Value::Bool(true));
        results.insert("vcruntime_note".to_string(), serde_json::Value::String("Install Visual C++ Redistributable manually from Microsoft if needed".to_string()));
    } else if cfg!(target_os = "macos") {
        append_to_log(&log_path, "macOS detected: trying brew to install python and ffmpeg");
        let mut cmd_brew_py = Command::new("brew");
        cmd_brew_py.args(&["install", "python"]);
        hide_console(&mut cmd_brew_py);
        let brew_py = cmd_brew_py.status();
        append_to_log(&log_path, &format!("brew python status: {:?}", brew_py));
        let mut cmd_brew_ff = Command::new("brew");
        cmd_brew_ff.args(&["install", "ffmpeg"]);
        hide_console(&mut cmd_brew_ff);
        let brew_ff = cmd_brew_ff.status();
        append_to_log(&log_path, &format!("brew ffmpeg status: {:?}", brew_ff));
        results.insert("python_attempted".to_string(), serde_json::Value::Bool(true));
        results.insert("ffmpeg_attempted".to_string(), serde_json::Value::Bool(true));
    } else {
        // Assume linux
        append_to_log(&log_path, "Linux detected: trying apt/dnf/pacman to install python3 and ffmpeg");
        let mut cmd_apt = Command::new("sh");
        cmd_apt.args(&["-c", "apt-get update && apt-get install -y python3 python3-pip ffmpeg"]);
        hide_console(&mut cmd_apt);
        let apt_update = cmd_apt.status();
        append_to_log(&log_path, &format!("apt status: {:?}", apt_update));
        let mut cmd_dnf = Command::new("sh");
        cmd_dnf.args(&["-c", "dnf install -y python3 python3-pip ffmpeg"]);
        hide_console(&mut cmd_dnf);
        let dnf = cmd_dnf.status();
        append_to_log(&log_path, &format!("dnf status: {:?}", dnf));
        let mut cmd_pac = Command::new("sh");
        cmd_pac.args(&["-c", "pacman -S --noconfirm python python-pip ffmpeg"]);
        hide_console(&mut cmd_pac);
        let pacman = cmd_pac.status();
        append_to_log(&log_path, &format!("pacman status: {:?}", pacman));
        results.insert("python_attempted".to_string(), serde_json::Value::Bool(true));
        results.insert("ffmpeg_attempted".to_string(), serde_json::Value::Bool(true));
    }

    append_to_log(&log_path, "System dependency installer finished (check OS package manager output above)");

    // Try installing Rust toolchain if pip builds require it
    append_to_log(&log_path, "Checking Rust toolchain (needed for building some Python wheels)...");
    let mut need_rust = true;
    let mut cmd_rustc = Command::new("rustc");
    cmd_rustc.arg("--version");
    hide_console(&mut cmd_rustc);
    if let Ok(status) = cmd_rustc.status() {
        if status.success() { need_rust = false; }
    }
    if need_rust {
        append_to_log(&log_path, "Rust not found - attempting to install rust toolchain...");
        if cfg!(windows) {
            let mut cmd_r = Command::new("winget");
            cmd_r.args(&["install", "--id", "RustLang.Rust", "-e", "--silent"]);
            hide_console(&mut cmd_r);
            let r = cmd_r.status();
            append_to_log(&log_path, &format!("winget rust status: {:?}", r));
        } else if cfg!(target_os = "macos") {
            let mut cmd_r = Command::new("brew");
            cmd_r.args(&["install", "rust"]);
            hide_console(&mut cmd_r);
            let r = cmd_r.status();
            append_to_log(&log_path, &format!("brew rust status: {:?}", r));
        } else {
            let mut cmd_r = Command::new("sh");
            cmd_r.args(&["-c", "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"]);
            hide_console(&mut cmd_r);
            let r = cmd_r.status();
            append_to_log(&log_path, &format!("rustup install status: {:?}", r));
        }
    } else {
        append_to_log(&log_path, "Rust toolchain already present");
    }

    Ok(serde_json::Value::Object(results))
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
