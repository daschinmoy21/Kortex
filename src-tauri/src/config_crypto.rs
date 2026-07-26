use std::fs;
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::Manager;
use keyring::Entry;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;

use crate::atomic_write_file;

// Keyring service for the encryption master key
const KEYRING_SERVICE_MASTER: &str = "Logia";
const KEYRING_USERNAME_MASTER: &str = "encryption_master_key";

/// Get or create the per-installation encryption key.
///
/// **Design**: keyring is the primary store (Windows Credential Manager,
/// macOS Keychain, Linux Secret Service).  Only if keyring `set_password`
/// fails do we fall back to a local file `.encryption_key` with restrictive
/// Unix permissions (0o600).  The file fallback is less secure because the
/// key lives on disk in the clear, so we emit a warning when it is used.
fn get_or_create_encryption_key(app_handle: &tauri::AppHandle) -> Result<[u8; 32], String> {
    // 1. Try to get existing key from keyring (preferred)
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

    // 2. Key not in keyring — check file fallback
    let config_dir = get_config_directory(app_handle)?;
    let key_file = config_dir.join(".encryption_key");
    
    if key_file.exists() {
        if let Ok(content) = fs::read_to_string(&key_file) {
            if let Ok(key_bytes) = general_purpose::STANDARD.decode(content.trim()) {
                if key_bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&key_bytes);
                    // Try to migrate into keyring so future reads can skip the file
                    let key_b64 = general_purpose::STANDARD.encode(&key);
                    if let Ok(entry) = Entry::new(KEYRING_SERVICE_MASTER, KEYRING_USERNAME_MASTER) {
                        let _ = entry.set_password(&key_b64);
                    }
                    // Ensure restrictive permissions even on pre-existing file
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600));
                    }
                    return Ok(key);
                }
            }
        }
    }

    // 3. No key exists — generate a new one
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    let key_b64 = general_purpose::STANDARD.encode(&key);
    
    // Try to store in keyring first
    let keyring_ok = match Entry::new(KEYRING_SERVICE_MASTER, KEYRING_USERNAME_MASTER) {
        Ok(entry) => entry.set_password(&key_b64).is_ok(),
        Err(_) => false,
    };
    
    if !keyring_ok {
        // Keyring unavailable — fall back to file with restrictive permissions
        eprintln!("WARNING: OS keyring unavailable. Storing encryption key in file fallback ({}). This is less secure.", key_file.display());
        println!("WARNING: OS keyring unavailable — using .encryption_key file fallback (less secure)");
        
        fs::write(&key_file, &key_b64)
            .map_err(|e| format!("Failed to store encryption key: {}", e))?;
        
        // Set restrictive permissions on Unix (owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600)) {
                eprintln!("WARNING: Could not set restrictive permissions on {}: {}", key_file.display(), e);
            }
        }
    }
    
    Ok(key)
}

pub(crate) fn encrypt_api_key(app_handle: &tauri::AppHandle, plaintext: &str) -> Result<String, String> {
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

pub(crate) fn decrypt_api_key(app_handle: &tauri::AppHandle, encrypted: &str) -> Result<String, String> {
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
pub async fn has_google_api_key(app_handle: tauri::AppHandle) -> Result<bool, String> {
    // Check keyring first
    if try_get_keyring(KEYRING_SERVICE, KEYRING_USERNAME).is_some() {
        return Ok(true);
    }

    // Check config.json for encrypted or legacy plain key
    let config_dir = get_config_directory(&app_handle)?;
    let config_file = config_dir.join("config.json");

    if config_file.exists() {
        if let Ok(content) = fs::read_to_string(&config_file) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                if config.get("encrypted_google_api_key").is_some()
                    || config.get("google_api_key").is_some()
                {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}

#[tauri::command]
pub async fn get_google_api_key(app_handle: tauri::AppHandle) -> Result<String, String> {
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
pub async fn save_google_api_key(key: String, app_handle: tauri::AppHandle) -> Result<(), String> {
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
pub async fn remove_google_api_key(app_handle: tauri::AppHandle) -> Result<(), String> {
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
