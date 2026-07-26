import { useState, useEffect } from 'react';
import { Dialog, DialogPanel, DialogTitle, Description } from '@headlessui/react';
import { invoke } from '@tauri-apps/api/core';
import useUiStore from '../store/UiStore';
import { X, RefreshCw, Check, Loader2, AlertTriangle, Download, Upload, GitBranch, Link2, Unlink } from 'lucide-react';
import toast from 'react-hot-toast';

// Git sync status from backend
interface GitSyncStatus {
  configured: boolean;
  remote_url: string | null;
  branch: string;
  dirty: boolean;
  ahead: number;
  behind: number;
  last_sync: string | null;
  git_available: boolean;
  message: string;
}

interface GitSyncResult {
  message: string;
  needs_reload: boolean;
}

export const Settings = () => {
  const {
    isSettingsOpen,
    setIsSettingsOpen,
    setGoogleApiKey,
    gitSyncConfigured,
    setGitSyncConfigured,
    isSyncing,
    setIsSyncing,
    lastSyncedAt,
    setLastSyncedAt
  } = useUiStore();
  const [apiKey, setApiKey] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [status, setStatus] = useState<'idle' | 'saving' | 'saved' | 'error'>('idle');
  const [isInstallingDeps, setIsInstallingDeps] = useState(false);
  const [installStatus, setInstallStatus] = useState<'idle' | 'installing' | 'installed' | 'error'>('idle');
  const [installLog, setInstallLog] = useState<string>('');
  // Git sync state
  const [remoteUrl, setRemoteUrl] = useState('');
  const [branchName, setBranchName] = useState('main');
  const [gitStatus, setGitStatus] = useState<GitSyncStatus | null>(null);

  useEffect(() => {
    if (isSettingsOpen) {
      loadApiKey();
      checkGitStatus();
    }
  }, [isSettingsOpen]);

  const checkGitStatus = async () => {
    try {
      const status = await invoke<GitSyncStatus>('git_sync_status');
      setGitStatus(status);
      setGitSyncConfigured(status.configured);
      if (status.remote_url) setRemoteUrl(status.remote_url);
      if (status.branch) setBranchName(status.branch);
      if (status.last_sync) setLastSyncedAt(new Date(status.last_sync));
    } catch (e) {
      console.error('Failed to check git status', e);
    }
  };

  const loadApiKey = async () => {
    try {
      const key = await invoke<string>('get_google_api_key');
      setApiKey(key);
      setGoogleApiKey(key);
    } catch (error) {
      console.error('Failed to load API key:', error);
    }
  };

  const handleSave = async () => {
    if (!apiKey.trim()) {
      setStatus('error');
      toast.error("API Key cannot be empty");
      return;
    }

    setIsLoading(true);
    setStatus('saving');
    try {
      await invoke('save_google_api_key', { key: apiKey });
      setGoogleApiKey(apiKey);
      setStatus('saved');
      toast.success('API Key saved successfully');
      setTimeout(() => setStatus('idle'), 2000);
    } catch (error) {
      console.error('Failed to save API key:', error);
      setStatus('error');
      toast.error('Failed to save API key');
    } finally {
      setIsLoading(false);
    }
  };

  const handleInstallTranscriptionDeps = async () => {
    setIsInstallingDeps(true);
    setInstallStatus('installing');
    try {
      await invoke('install_transcription_dependencies');
      setInstallStatus('installed');
      setTimeout(() => setInstallStatus('idle'), 2000);
    } catch (error) {
      console.error('Failed to install transcription dependencies:', error);
      setInstallStatus('error');
    } finally {
      setIsInstallingDeps(false);
    }
  };

  const handleRemoveApiKey = async () => {
    setIsLoading(true);
    try {
      await invoke('remove_google_api_key');
      setApiKey('');
      setGoogleApiKey('');
      setStatus('saved');
      toast.success('API key removed successfully!');
      setTimeout(() => setStatus('idle'), 2000);
    } catch (error) {
      console.error('Failed to remove API key:', error);
      setStatus('error');
      toast.error('Failed to remove API key.');
    } finally {
      setIsLoading(false);
    }
  };

  // Git sync actions
  const handleGitConfigure = async () => {
    if (!remoteUrl.trim()) {
      toast.error('Remote URL is required');
      return;
    }
    setIsSyncing(true);
    try {
      const result = await invoke<GitSyncStatus>('git_sync_configure', {
        remoteUrl: remoteUrl.trim(),
        branch: branchName.trim() || 'main',
      });
      setGitStatus(result);
      setGitSyncConfigured(result.configured);
      toast.success('Git sync configured!');
    } catch (e) {
      toast.error(`Configure failed: ${e}`);
    } finally {
      setIsSyncing(false);
    }
  };

  const handleGitSyncNow = async () => {
    setIsSyncing(true);
    try {
      const result = await invoke<GitSyncResult>('git_sync_now');
      toast.success(result.message);
      setLastSyncedAt(new Date());
      await checkGitStatus();
      if (result.needs_reload) {
        const { loadNotes, loadFolders } = (await import('../store/notesStore')).useNotesStore.getState();
        await loadNotes();
        await loadFolders();
      }
    } catch (e) {
      toast.error(`Sync failed: ${e}`);
    } finally {
      setIsSyncing(false);
    }
  };

  const handleGitForcePull = async () => {
    if (!confirm('This will discard ALL local changes and replace them with the remote state. Continue?')) return;
    setIsSyncing(true);
    try {
      const result = await invoke<GitSyncResult>('git_sync_force_pull');
      toast.success(result.message);
      setLastSyncedAt(new Date());
      await checkGitStatus();
      // Reload notes
      const { loadNotes, loadFolders } = (await import('../store/notesStore')).useNotesStore.getState();
      await loadNotes();
      await loadFolders();
    } catch (e) {
      toast.error(`Force pull failed: ${e}`);
    } finally {
      setIsSyncing(false);
    }
  };

  const handleGitForcePush = async () => {
    if (!confirm('This will force-push your local state to the remote, overwriting remote changes. Continue?')) return;
    setIsSyncing(true);
    try {
      const result = await invoke<GitSyncResult>('git_sync_force_push');
      toast.success(result.message);
      setLastSyncedAt(new Date());
      await checkGitStatus();
    } catch (e) {
      toast.error(`Force push failed: ${e}`);
    } finally {
      setIsSyncing(false);
    }
  };

  const handleGitDisconnect = async () => {
    setIsSyncing(true);
    try {
      const msg = await invoke<string>('git_sync_disconnect');
      toast.success(msg);
      setGitSyncConfigured(false);
      setGitStatus(null);
      setLastSyncedAt(null);
      setRemoteUrl('');
    } catch (e) {
      toast.error(`Disconnect failed: ${e}`);
    } finally {
      setIsSyncing(false);
    }
  };

  // Poll the backend install log while installation is running so the UI can show progress
  useEffect(() => {
    let timer: number | undefined;
    async function poll() {
      try {
        const content = await invoke<string>('read_install_log');
        setInstallLog(content);
      } catch (e) {
        // ignore errors while polling
      }
    }

    if (isInstallingDeps) {
      poll();
      timer = window.setInterval(poll, 1000);
    } else {
      poll();
    }

    return () => {
      if (timer) clearInterval(timer);
    };
  }, [isInstallingDeps]);

  return (
    <>
      <Dialog
        open={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        className="relative z-[1000]"
      >
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm" aria-hidden="true" />

        <div className="fixed inset-0 flex items-center justify-center p-4">
          <DialogPanel className="w-full max-w-md rounded-xl bg-zinc-900 border border-zinc-800 p-6 shadow-xl">
            <div className="flex items-center justify-between mb-6">
              <DialogTitle className="text-lg font-medium text-white">Settings</DialogTitle>
              <button
                onClick={() => setIsSettingsOpen(false)}
                className="text-zinc-400 hover:text-white transition-colors"
              >
                <X size={20} />
              </button>
            </div>

            <div className="space-y-6">
              <div>
                <h3 className="text-sm font-medium text-zinc-300 mb-2">General</h3>
                <div className="space-y-4">
                  <div>
                    <label htmlFor="apiKey" className="block text-xs text-zinc-400 mb-1.5">
                      Gemini API Key
                    </label>
                    <Description className="text-xs text-zinc-500 mb-2">
                      Required for AI features. The key is stored locally on your device.
                    </Description>
                    <input
                      id="apiKey"
                      type="password"
                      value={apiKey}
                      onChange={(e) => setApiKey(e.target.value)}
                      placeholder="Enter your API key"
                      className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-zinc-600 focus:ring-1 focus:ring-zinc-600 transition-colors"
                    />
                  </div>
                </div>
              </div>

              <div>
                <h3 className="text-sm font-medium text-zinc-300 mb-2">Transcription</h3>
                <div className="space-y-4">
                  <div>
                    <Description className="text-xs text-zinc-500 mb-2">
                      Install Python dependencies required for audio transcription. This will download faster-whisper and other required packages.
                    </Description>
                    <div className="flex items-center gap-3">
                      <div className="flex-1">
                        <button
                          onClick={handleInstallTranscriptionDeps}
                          disabled={isInstallingDeps}
                          className={`
                         px-4 py-2 rounded-lg text-sm font-medium transition-all
                         ${installStatus === 'installed'
                              ? 'bg-green-600 text-white hover:bg-green-700'
                              : installStatus === 'error'
                                ? 'bg-red-600 text-white hover:bg-red-700'
                                : 'bg-zinc-100 text-zinc-900 hover:bg-zinc-200 disabled:opacity-50 disabled:cursor-not-allowed'}
                       `}
                        >
                          {isInstallingDeps ? 'Installing...' :
                            installStatus === 'installed' ? 'Installed ✓' :
                              installStatus === 'error' ? 'Error' : 'Install Dependencies'}
                        </button>
                      </div>
                      <div className="w-48 text-xs text-zinc-400">
                        {isInstallingDeps ? 'Downloading / installing...' : ''}
                      </div>
                    </div>
                    <div className="mt-3">
                      <pre className="max-h-40 overflow-auto text-xs bg-zinc-900 border border-zinc-800 p-2 rounded text-zinc-300">{installLog}</pre>
                    </div>
                  </div>
                </div>
              </div>

              {/* Git Sync Section */}
              <div className="bg-gradient-to-br from-zinc-800/50 to-zinc-900/50 rounded-xl p-4 border border-zinc-700/50">
                <div className="flex items-center gap-3 mb-4">
                  <div className={`p-2 rounded-lg ${gitSyncConfigured ? 'bg-green-500/20' : 'bg-zinc-700/50'}`}>
                    <GitBranch className={`w-5 h-5 ${gitSyncConfigured ? 'text-green-400' : 'text-zinc-400'}`} />
                  </div>
                  <div>
                    <h3 className="text-sm font-semibold text-zinc-200">Git Sync</h3>
                    <p className="text-xs text-zinc-500">
                      {gitSyncConfigured ? 'Notes are synced via git' : 'Sync notes using a git remote'}
                    </p>
                  </div>
                </div>

                {/* Git not available warning */}
                {gitStatus && !gitStatus.git_available && (
                  <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-3 mb-3">
                    <div className="flex items-center gap-2">
                      <AlertTriangle className="w-4 h-4 text-amber-400" />
                      <span className="text-xs text-amber-400">git not found on PATH. Install git to enable sync.</span>
                    </div>
                  </div>
                )}

                {/* Configuration inputs (always visible, simpler flow) */}
                <div className="space-y-3 mb-3">
                  <div>
                    <label className="block text-xs text-zinc-400 mb-1">Remote URL</label>
                    <input
                      type="text"
                      value={remoteUrl}
                      onChange={(e) => setRemoteUrl(e.target.value)}
                      placeholder="git@github.com:user/logia-notes.git"
                      className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-zinc-400 mb-1">Branch</label>
                    <input
                      type="text"
                      value={branchName}
                      onChange={(e) => setBranchName(e.target.value)}
                      placeholder="main"
                      className="w-full bg-zinc-950 border border-zinc-800 rounded-lg px-3 py-2 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-zinc-600 transition-colors"
                    />
                  </div>
                </div>

                {/* Status bar */}
                {gitSyncConfigured && gitStatus && (
                  <div className="flex items-center justify-between px-3 py-2 bg-zinc-800/50 rounded-lg mb-3">
                    <div className="flex items-center gap-2">
                      <Check className="w-4 h-4 text-green-400" />
                      <span className="text-xs text-zinc-300">
                        {gitStatus.branch}
                        {gitStatus.ahead > 0 && <span className="text-blue-400 ml-1">↑{gitStatus.ahead}</span>}
                        {gitStatus.behind > 0 && <span className="text-amber-400 ml-1">↓{gitStatus.behind}</span>}
                        {gitStatus.dirty && <span className="text-yellow-400 ml-1">•</span>}
                      </span>
                    </div>
                    <span className="text-xs text-zinc-500">
                      {lastSyncedAt ? `Last sync: ${lastSyncedAt.toLocaleTimeString()}` : 'Not synced yet'}
                    </span>
                  </div>
                )}

                {/* Action buttons */}
                <div className="space-y-2">
                  {!gitSyncConfigured ? (
                    <button
                      onClick={handleGitConfigure}
                      disabled={isSyncing || !remoteUrl.trim()}
                      className="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50 transition-all"
                    >
                      {isSyncing ? (
                        <><Loader2 className="w-4 h-4 animate-spin" /> Configuring...</>
                      ) : (
                        <><Link2 className="w-4 h-4" /> Connect Remote</>
                      )}
                    </button>
                  ) : (
                    <>
                      <div className="flex gap-2">
                        <button
                          onClick={handleGitSyncNow}
                          disabled={isSyncing}
                          className="flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-lg text-sm font-medium bg-zinc-100 text-zinc-900 hover:bg-white disabled:opacity-50 transition-all"
                        >
                          {isSyncing ? (
                            <><RefreshCw className="w-4 h-4 animate-spin" /> Syncing...</>
                          ) : (
                            <><RefreshCw className="w-4 h-4" /> Sync Now</>
                          )}
                        </button>
                        <button
                          onClick={handleGitConfigure}
                          disabled={isSyncing}
                          className="px-4 py-2 rounded-lg text-sm font-medium bg-zinc-700 text-zinc-200 hover:bg-zinc-600 disabled:opacity-50 transition-all"
                          title="Update remote URL"
                        >
                          <Link2 className="w-4 h-4" />
                        </button>
                      </div>

                      {/* Force pull / push */}
                      <div className="flex gap-2">
                        <button
                          onClick={handleGitForcePull}
                          disabled={isSyncing}
                          className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium bg-amber-600 text-white hover:bg-amber-500 disabled:opacity-50 transition-all"
                          title="Discard local changes, use remote"
                        >
                          <Download className="w-3 h-3" />
                          Use Remote
                        </button>
                        <button
                          onClick={handleGitForcePush}
                          disabled={isSyncing}
                          className="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium bg-zinc-600 text-white hover:bg-zinc-500 disabled:opacity-50 transition-all"
                          title="Overwrite remote with local"
                        >
                          <Upload className="w-3 h-3" />
                          Use Local
                        </button>
                      </div>

                      {/* Disconnect */}
                      <button
                        onClick={handleGitDisconnect}
                        disabled={isSyncing}
                        className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg text-xs font-medium text-zinc-400 hover:text-red-400 hover:bg-red-500/10 transition-all"
                      >
                        <Unlink className="w-3 h-3" />
                        Disconnect Remote
                      </button>
                    </>
                  )}
                </div>
              </div>

              <div className="flex justify-end pt-4 border-t border-zinc-800">
                {apiKey && (
                  <button
                    onClick={handleRemoveApiKey}
                    disabled={isLoading}
                    className="mr-2 px-4 py-2 rounded-lg text-sm font-medium transition-all bg-red-600 text-white hover:bg-red-700 disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    Remove API Key
                  </button>
                )}
                <button
                  onClick={handleSave}
                  disabled={isLoading || !apiKey}
                  className={`px-4 py-2 rounded-lg text-sm font-medium transition-all ${
                    status === 'saved'
                      ? 'bg-green-600 text-white hover:bg-green-700'
                      : status === 'error'
                        ? 'bg-red-600 text-white hover:bg-red-700'
                        : 'bg-zinc-100 text-zinc-900 hover:bg-zinc-200 disabled:opacity-50 disabled:cursor-not-allowed'
                  }`}
                >
                  {status === 'saving' ? 'Saving...' :
                    status === 'saved' ? 'Saved!' :
                      status === 'error' ? 'Error' : 'Save Changes'}
                </button>
              </div>
            </div>
          </DialogPanel>
        </div>
      </Dialog>
    </>
  );
};
