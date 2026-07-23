use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use tauri::path::BaseDirectory;
use tauri::Manager;

/// Hide console windows on Windows when spawning subprocesses
#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW
        cmd.creation_flags(0x08000000);
    }
}

/// Simple state for git sync (holds last sync timestamp in memory)
pub struct GitSyncState {
    pub last_sync: Mutex<Option<String>>,
}

impl GitSyncState {
    pub fn new() -> Self {
        Self {
            last_sync: Mutex::new(None),
        }
    }
}

// ── Path helpers ────────────────────────────────────────────────────────────

/// Resolve the Logia root directory (~/Documents/Logia or XDG fallback).
/// This is the parent of notes/, folders/, kanban/, trash/.
fn resolve_logia_root(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir_result = app_handle
        .path()
        .resolve("Logia", BaseDirectory::Document);

    let target_dir: PathBuf = match dir_result {
        Ok(path) => path,
        Err(_) => {
            let home_dir = app_handle
                .path()
                .resolve("", BaseDirectory::Home)
                .map_err(|_| "Could not resolve home directory")?;
            home_dir.join("Documents").join("Logia")
        }
    };

    // Create if it doesn't exist
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create Logia directory: {}", e))?;
    }

    Ok(target_dir)
}

/// Read or write the sync meta file (.logia-sync-meta.json) inside the Logia root
fn sync_meta_path(root: &PathBuf) -> PathBuf {
    root.join(".logia-sync-meta.json")
}

fn read_sync_meta(root: &PathBuf) -> serde_json::Value {
    let p = sync_meta_path(root);
    if p.exists() {
        fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

fn write_sync_meta(root: &PathBuf, meta: &serde_json::Value) {
    let p = sync_meta_path(root);
    if let Ok(s) = serde_json::to_string_pretty(meta) {
        let _ = fs::write(&p, s);
    }
}

// ── Git helper ───────────────────────────────────────────────────────────────

fn run_git(root: &PathBuf, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args(args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_console(&mut cmd);

    let output = cmd.output().map_err(|e| format!("Failed to run git: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

/// Run git, but return (success, stdout, stderr) — used when we need to inspect failure modes
fn run_git_raw(root: &PathBuf, args: &[&str]) -> (bool, String, String) {
    let mut cmd = Command::new("git");
    cmd.current_dir(root);
    cmd.args(args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    hide_console(&mut cmd);

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            (output.status.success(), stdout, stderr)
        }
        Err(_) => (false, String::new(), "git command failed to start".to_string()),
    }
}

fn git_available() -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("--version");
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    hide_console(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn has_git_repo(root: &PathBuf) -> bool {
    root.join(".git").exists()
}

fn get_branch(root: &PathBuf) -> String {
    run_git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|_| "main".to_string())
}

fn has_origin(root: &PathBuf) -> bool {
    run_git(root, &["remote", "get-url", "origin"]).is_ok()
}

fn get_origin_url(root: &PathBuf) -> Option<String> {
    run_git(root, &["remote", "get-url", "origin"]).ok()
}

fn is_dirty(root: &PathBuf) -> bool {
    let (ok, stdout, _) = run_git_raw(root, &["status", "--porcelain"]);
    ok && !stdout.is_empty()
}

fn count_ahead_behind(root: &PathBuf, branch: &str) -> (usize /*ahead*/, usize /*behind*/) {
    let (ok, stdout, _) = run_git_raw(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("origin/{}...{}", branch, branch),
        ],
    );
    if !ok {
        return (0, 0);
    }
    let parts: Vec<&str> = stdout.split_whitespace().collect();
    if parts.len() >= 2 {
        (
            parts[1].parse::<usize>().unwrap_or(0),  // ahead
            parts[0].parse::<usize>().unwrap_or(0),  // behind
        )
    } else {
        (0, 0)
    }
}

/// Create a sensible .gitignore if missing
fn ensure_gitignore(root: &PathBuf) -> Result<(), String> {
    let gitignore = root.join(".gitignore");
    if !gitignore.exists() {
        let contents = "# Logia git sync — ignore temporary and runtime files\n\
                        *.tmp\n\
                        .encryption_key\n\
                        .logia-sync-meta.json\n";
        fs::write(&gitignore, contents)
            .map_err(|e| format!("Failed to write .gitignore: {}", e))?;
    }
    Ok(())
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitSyncStatus {
    pub configured: bool,
    pub remote_url: Option<String>,
    pub branch: String,
    pub dirty: bool,
    pub ahead: usize,
    pub behind: usize,
    pub last_sync: Option<String>,
    pub git_available: bool,
    pub message: String,
}

#[tauri::command]
pub async fn git_sync_status(app_handle: tauri::AppHandle) -> Result<GitSyncStatus, String> {
    let root = resolve_logia_root(&app_handle)?;
    let git_ok = git_available();

    if !git_ok {
        return Ok(GitSyncStatus {
            configured: false,
            remote_url: None,
            branch: String::new(),
            dirty: false,
            ahead: 0,
            behind: 0,
            last_sync: None,
            git_available: false,
            message: "git not found on PATH. Install git to enable sync.".to_string(),
        });
    }

    if !has_git_repo(&root) {
        return Ok(GitSyncStatus {
            configured: false,
            remote_url: None,
            branch: String::new(),
            dirty: false,
            ahead: 0,
            behind: 0,
            last_sync: None,
            git_available: true,
            message: "No git repository. Configure a remote to start syncing.".to_string(),
        });
    }

    let configured = has_origin(&root);
    let remote_url = get_origin_url(&root);
    let branch = get_branch(&root);
    let dirty = is_dirty(&root);
    let (ahead, behind) = if configured {
        count_ahead_behind(&root, &branch)
    } else {
        (0, 0)
    };

    let meta = read_sync_meta(&root);
    let last_sync = meta
        .get("last_sync")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(GitSyncStatus {
        configured,
        remote_url,
        branch,
        dirty,
        ahead,
        behind,
        last_sync,
        git_available: true,
        message: if configured {
            "Git sync is configured.".to_string()
        } else {
            "Set a remote URL to enable sync.".to_string()
        },
    })
}

#[tauri::command]
pub async fn git_sync_configure(
    remote_url: String,
    branch: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<GitSyncStatus, String> {
    if !git_available() {
        return Err("git not found on PATH. Install git to enable sync.".to_string());
    }

    let root = resolve_logia_root(&app_handle)?;
    let branch = branch.unwrap_or_else(|| "main".to_string());

    // Validate the URL minimally (must not be empty, should look like a git URL)
    if remote_url.trim().is_empty() {
        return Err("Remote URL cannot be empty".to_string());
    }

    // Initialize git repo if needed
    if !has_git_repo(&root) {
        run_git(&root, &["init"])?;
        // Set default branch name
        run_git(&root, &["checkout", "-b", &branch])?;
    }

    // Ensure .gitignore
    ensure_gitignore(&root)?;

    // Set local git identity if unset
    let (ok, _, _) = run_git_raw(&root, &["config", "user.name"]);
    if !ok {
        run_git(&root, &["config", "user.name", "Logia"])?;
    }
    let (ok, _, _) = run_git_raw(&root, &["config", "user.email"]);
    if !ok {
        run_git(&root, &["config", "user.email", "logia@localhost"])?;
    }

    // Set or replace origin remote
    if has_origin(&root) {
        run_git(&root, &["remote", "remove", "origin"])?;
    }
    run_git(&root, &["remote", "add", "origin", &remote_url])?;

    // Checkout/create branch
    let current_branch = get_branch(&root);
    if current_branch != branch {
        // Try to checkout; if branch doesn't exist, create it
        if run_git(&root, &["checkout", &branch]).is_err() {
            run_git(&root, &["checkout", "-b", &branch])?;
        }
    }

    // Initial commit of existing files if nothing committed yet
    let (ok, stdout, _) = run_git_raw(&root, &["rev-list", "--count", "HEAD"]);
    let commit_count: usize = if ok { stdout.parse().unwrap_or(0) } else { 0 };

    if commit_count == 0 {
        // Check if there are any files to commit
        let (_, status_out, _) = run_git_raw(&root, &["status", "--porcelain"]);
        if !status_out.is_empty() {
            run_git(&root, &["add", "-A"])?;
            run_git(&root, &["commit", "-m", "Initial Logia notes"])?;
        } else {
            // Allow empty initial commit so we have a HEAD
            run_git(&root, &["commit", "--allow-empty", "-m", "Initial Logia notes"])?;
        }
    }

    // Record configuration time
    let now = chrono::Utc::now().to_rfc3339();
    let mut meta = read_sync_meta(&root);
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "configured_at".to_string(),
            serde_json::Value::String(now.clone()),
        );
    }
    write_sync_meta(&root, &meta);

    // Return fresh status
    git_sync_status(app_handle).await
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GitSyncResult {
    pub message: String,
    pub needs_reload: bool,
}

#[tauri::command]
pub async fn git_sync_now(app_handle: tauri::AppHandle) -> Result<GitSyncResult, String> {
    let root = resolve_logia_root(&app_handle)?;

    if !git_available() {
        return Err("git not found on PATH".to_string());
    }

    if !has_git_repo(&root) {
        return Err("No git repository. Configure sync first.".to_string());
    }

    if !has_origin(&root) {
        return Err("No remote configured. Set a remote URL first.".to_string());
    }

    let branch = get_branch(&root);

    // Stage all changes
    run_git(&root, &["add", "-A"])?;

    // Commit if dirty
    let dirty_before = is_dirty(&root);
    if dirty_before {
        let now = chrono::Utc::now().to_rfc3339();
        run_git(&root, &["commit", "-m", &format!("Logia sync {}", now)])?;
    }

    // Capture HEAD before pull
    let head_before = run_git(&root, &["rev-parse", "HEAD"]).unwrap_or_default();

    // Pull with rebase
    let pull_result = run_git(&root, &["pull", "--rebase", "origin", &branch]);

    let mut needs_reload = false;

    match pull_result {
        Ok(_) => {
            // Pull succeeded — check if HEAD changed
            let head_after = run_git(&root, &["rev-parse", "HEAD"]).unwrap_or_default();
            needs_reload = head_before != head_after;
        }
        Err(e) => {
            let lower = e.to_lowercase();
            // Check if remote has no branch yet (first push scenario)
            if lower.contains("couldn't find remote ref")
                || lower.contains("couldn't find remote branch")
                || lower.contains("no tracking")
            {
                // No remote branch yet — skip pull, proceed to push (first sync)
            } else if lower.contains("conflict")
                || lower.contains("unmerged")
                || lower.contains("would be overwritten")
            {
                // Abort the rebase if in progress
                let _ = run_git(&root, &["rebase", "--abort"]);
                return Err(format!(
                    "Merge conflict detected. Resolve manually in your notes folder ({}), \
                     or use 'Use Remote' to discard local changes, \
                     or 'Use Local' to force push your changes.\n\nGit error: {}",
                    root.display(),
                    e
                ));
            } else {
                return Err(format!("Pull failed: {}", e));
            }
        }
    }

    // Push
    run_git(&root, &["push", "-u", "origin", &branch])
        .map_err(|e| format!("Push failed: {}. Check your remote URL and authentication.", e))?;

    // Record last sync time
    let now = chrono::Utc::now().to_rfc3339();
    let mut meta = read_sync_meta(&root);
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "last_sync".to_string(),
            serde_json::Value::String(now),
        );
    }
    write_sync_meta(&root, &meta);

    // Check if we're clean after sync
    let dirty_after = is_dirty(&root);

    Ok(GitSyncResult {
        message: if dirty_before || dirty_after {
            "Sync complete. Changes were pushed.".to_string()
        } else {
            "Everything is already in sync.".to_string()
        },
        needs_reload,
    })
}

#[tauri::command]
pub async fn git_sync_force_pull(app_handle: tauri::AppHandle) -> Result<GitSyncResult, String> {
    let root = resolve_logia_root(&app_handle)?;

    if !git_available() {
        return Err("git not found on PATH".to_string());
    }

    if !has_origin(&root) {
        return Err("No remote configured.".to_string());
    }

    let branch = get_branch(&root);

    // Fetch latest from remote
    run_git(&root, &["fetch", "origin"])?;

    // Hard reset to origin/branch (destroys local changes)
    run_git(&root, &["reset", "--hard", &format!("origin/{}", branch)])
        .map_err(|e| format!("Failed to reset to remote: {}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut meta = read_sync_meta(&root);
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "last_sync".to_string(),
            serde_json::Value::String(now),
        );
    }
    write_sync_meta(&root, &meta);

    Ok(GitSyncResult {
        message: "Local state reset to match remote. Reloading notes...".to_string(),
        needs_reload: true,
    })
}

#[tauri::command]
pub async fn git_sync_force_push(app_handle: tauri::AppHandle) -> Result<GitSyncResult, String> {
    let root = resolve_logia_root(&app_handle)?;

    if !git_available() {
        return Err("git not found on PATH".to_string());
    }

    if !has_origin(&root) {
        return Err("No remote configured.".to_string());
    }

    let branch = get_branch(&root);

    // Stage all and commit if needed
    run_git(&root, &["add", "-A"])?;
    if is_dirty(&root) {
        let now = chrono::Utc::now().to_rfc3339();
        run_git(&root, &["commit", "-m", &format!("Logia sync {}", now)])?;
    }

    // Force push with lease (safer than --force)
    run_git(
        &root,
        &["push", "--force-with-lease", "origin", &branch],
    )
    .map_err(|e| format!("Force push failed: {}", e))?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut meta = read_sync_meta(&root);
    if let Some(obj) = meta.as_object_mut() {
        obj.insert(
            "last_sync".to_string(),
            serde_json::Value::String(now),
        );
    }
    write_sync_meta(&root, &meta);

    Ok(GitSyncResult {
        message: "Local state force-pushed to remote.".to_string(),
        needs_reload: false,
    })
}

#[tauri::command]
pub async fn git_sync_disconnect(app_handle: tauri::AppHandle) -> Result<String, String> {
    let root = resolve_logia_root(&app_handle)?;

    if has_origin(&root) {
        run_git(&root, &["remote", "remove", "origin"])?;
    }

    // Update meta
    let mut meta = read_sync_meta(&root);
    if let Some(obj) = meta.as_object_mut() {
        obj.remove("last_sync");
        obj.remove("configured_at");
    }
    write_sync_meta(&root, &meta);

    Ok("Disconnected from git remote. Local history is preserved.".to_string())
}
