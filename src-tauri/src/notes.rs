use chrono;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::Manager;
use uuid::Uuid;

use crate::atomic_write_file;

/// Validate that a user-supplied id is safe to use as a filename segment.
/// Rejects path separators, parent traversal, and other unexpected characters.
pub(crate) fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub(crate) fn require_safe_id(id: &str) -> Result<(), String> {
    if is_safe_id(id) {
        Ok(())
    } else {
        Err(format!("Invalid id: contains disallowed characters or path components"))
    }
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

fn get_kanban_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "kanban")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

fn get_trash_directory(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    resolve_logia_dir(app_handle, "trash")
        .map_err(|_| "Could not find document directory (checked XDG and Home fallback)".to_string())
}

#[tauri::command]
pub async fn get_notes_path(app_handle: tauri::AppHandle) -> Result<String, String> {
    let notes_dir = get_notes_directory(&app_handle)?;
    Ok(notes_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn create_note(
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
pub async fn get_all_notes(app_handle: tauri::AppHandle) -> Result<Vec<Note>, String> {
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
pub async fn save_note(note: Note, app_handle: tauri::AppHandle) -> Result<(), String> {
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
pub async fn toggle_star_note(note_id: String, app_handle: tauri::AppHandle) -> Result<bool, String> {
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
pub async fn delete_note(note_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
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
pub async fn create_folder(
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
pub async fn get_all_folders(app_handle: tauri::AppHandle) -> Result<Vec<Folder>, String> {
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
pub async fn update_folder(folder: Folder, app_handle: tauri::AppHandle) -> Result<(), String> {
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

/// Collect all descendant folder IDs for a given folder via BFS on parent_id chain.
fn collect_descendant_folder_ids(folders_dir: &PathBuf, root_id: &str) -> Vec<String> {
    let mut all_folders: Vec<Folder> = Vec::new();
    if let Ok(entries) = fs::read_dir(folders_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(folder) = serde_json::from_str::<Folder>(&content) {
                        all_folders.push(folder);
                    }
                }
            }
        }
    }
    // BFS: start from root_id, collect all children recursively
    let mut descendants: Vec<String> = Vec::new();
    let mut queue: Vec<String> = all_folders
        .iter()
        .filter(|f| f.parent_id.as_deref() == Some(root_id))
        .map(|f| f.id.clone())
        .collect();
    while let Some(current) = queue.pop() {
        descendants.push(current.clone());
        let children: Vec<String> = all_folders
            .iter()
            .filter(|f| f.parent_id.as_deref() == Some(&current))
            .map(|f| f.id.clone())
            .collect();
        queue.extend(children);
    }
    descendants
}

#[tauri::command]
pub async fn delete_folder(folder_id: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    require_safe_id(&folder_id)?;
    let folders_dir = get_folders_directory(&app_handle)?;
    let notes_dir = get_notes_directory(&app_handle)?;
    let trash_dir = get_trash_directory(&app_handle)?;

    // 1. Collect all descendant folder IDs via parent_id chain
    let descendant_ids = collect_descendant_folder_ids(&folders_dir, &folder_id);

    // Build the full set: the folder itself + all descendants
    let mut all_affected_folder_ids = descendant_ids.clone();
    all_affected_folder_ids.push(folder_id.clone());

    // 2. Trash all notes whose folder_id is any affected folder
    if let Ok(entries) = fs::read_dir(&notes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(note) = serde_json::from_str::<Note>(&content) {
                    if note.folder_id.as_ref().map_or(false, |fid| all_affected_folder_ids.contains(fid)) {
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

    // 3. Trash all descendant folders (deepest-first not required — any order works)
    for desc_id in &descendant_ids {
        let src = folders_dir.join(format!("{}.json", desc_id));
        let dst = trash_dir.join(format!("folder_{}.json", desc_id));
        if src.exists() {
            if fs::rename(&src, &dst).is_err() {
                let _ = fs::copy(&src, &dst);
                let _ = fs::remove_file(&src);
            }
        }
    }

    // 4. Trash F itself
    let source_path = folders_dir.join(format!("{}.json", folder_id));
    let dest_path = trash_dir.join(format!("folder_{}.json", folder_id));
    if let Err(_) = fs::rename(&source_path, &dest_path) {
        fs::copy(&source_path, &dest_path)
            .map_err(|e| format!("Failed to copy folder to trash: {}", e))?;
        fs::remove_file(&source_path)
            .map_err(|e| format!("Failed to remove original folder: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_kanban_data(app_handle: tauri::AppHandle) -> Result<Vec<KanbanTask>, String> {
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
pub async fn save_kanban_data(tasks: Vec<KanbanTask>, app_handle: tauri::AppHandle) -> Result<(), String> {
    let kanban_dir = get_kanban_directory(&app_handle)?;
    let file_path = kanban_dir.join("data.json");

    let data_json = serde_json::to_string_pretty(&tasks)
        .map_err(|e| format!("Failed to serialize kanban data: {}", e))?;

    atomic_write_file(&file_path, &data_json)?;

    Ok(())
}

// --- Trash Management Commands ---

#[tauri::command]
pub async fn get_trash_items(app_handle: tauri::AppHandle) -> Result<Vec<TrashItem>, String> {
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
pub async fn empty_trash(app_handle: tauri::AppHandle) -> Result<usize, String> {
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
pub async fn restore_from_trash(item_id: String, item_type: String, app_handle: tauri::AppHandle) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_id_valid_uuid() {
        assert!(is_safe_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(require_safe_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn safe_id_plain_alphanumeric() {
        assert!(is_safe_id("note123"));
        assert!(is_safe_id("my_folder_2024"));
        assert!(is_safe_id("abc-def_ghi"));
    }

    #[test]
    fn safe_id_rejects_empty() {
        assert!(!is_safe_id(""));
        assert!(require_safe_id("").is_err());
    }

    #[test]
    fn safe_id_rejects_parent_traversal() {
        assert!(!is_safe_id("../etc/passwd"));
        assert!(!is_safe_id(".."));
        assert!(!is_safe_id("foo/../bar"));
    }

    #[test]
    fn safe_id_rejects_path_separators() {
        assert!(!is_safe_id("a/b"));
        assert!(!is_safe_id("a\\b"));
        assert!(!is_safe_id("folder/note"));
    }

    #[test]
    fn safe_id_rejects_too_long() {
        let long_id = "a".repeat(129);
        assert!(!is_safe_id(&long_id));
        // 128 chars is the max
        let max_id = "a".repeat(128);
        assert!(is_safe_id(&max_id));
    }

    #[test]
    fn safe_id_rejects_unicode_and_special_chars() {
        assert!(!is_safe_id("héllo"));
        assert!(!is_safe_id("note name"));
        assert!(!is_safe_id("note.name"));
        assert!(!is_safe_id("note$name"));
        assert!(!is_safe_id("<script>"));
    }
}
