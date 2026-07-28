import { useNotesStore } from "@/store/notesStore";
import { motion } from "framer-motion";
import { FileText, Clock, Layout, ArrowRight, GitBranch, RefreshCw, Loader2 } from "lucide-react";
import toast from "react-hot-toast";
import { invoke } from "@tauri-apps/api/core";
import useUiStore from "../store/UiStore";
import { recentNotes as getRecentNotes } from "../lib/note-utils";
import { searchModKeyLabel } from "../lib/utils";

interface GitSyncResult {
  message: string;
  needs_reload: boolean;
}

export function EmptyState() {
  const { createNote, notes } = useNotesStore();
  const recent = getRecentNotes(notes, 3);
  const { gitSyncConfigured, isSyncing, setIsSyncing, setLastSyncedAt } = useUiStore();
  const modKey = searchModKeyLabel();

  const quickActions = [
    {
      icon: FileText,
      label: "New Note",
      description: "Start writing",
      action: () => createNote("text"),
      color: "blue",
    },
    {
      icon: Layout,
      label: "New Canvas",
      description: "Visual workspace",
      action: () => createNote("canvas"),
      color: "purple",
    },
  ];

  return (
    <div className="flex-1 flex flex-col items-center bg-zinc-950 h-full overflow-y-auto min-h-0">
      <div className="flex flex-col items-center justify-center min-h-full w-full px-8 py-12">
        {/* Hero Section */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5 }}
          className="text-center mb-12"
        >
          <h1 className="text-3xl font-bold text-zinc-100 mb-3 tracking-tight">
            Welcome to <span className="text-blue-400">Logia</span>
          </h1>
          <p className="text-zinc-500 text-sm max-w-md">
            Your intelligent notes companion. Create a note or use AI-powered
            voice capture to summarize your lectures and meetings.
          </p>
        </motion.div>

        {/* Quick Actions */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, delay: 0.1 }}
          className="flex gap-3 mb-12"
        >
          {quickActions.map((action, index) => (
            <button
              key={index}
              onClick={action.action || undefined}
              className={`group relative flex flex-col items-center gap-3 p-5 rounded-xl bg-zinc-900/50 border border-zinc-800 hover:border-zinc-700 hover:bg-zinc-800/50 transition-all duration-200 w-36`}
            >
              <div
                className={`p-3 rounded-lg transition-colors ${action.color === "blue"
                  ? "bg-blue-500/10 text-blue-400 group-hover:bg-blue-500/20"
                  : action.color === "purple"
                    ? "bg-purple-500/10 text-purple-400 group-hover:bg-purple-500/20"
                    : "bg-amber-500/10 text-amber-400 group-hover:bg-amber-500/20"
                  }`}
              >
                <action.icon size={22} />
              </div>
              <div className="text-center">
                <div className="text-sm font-medium text-zinc-200 group-hover:text-zinc-100 transition-colors">
                  {action.label}
                </div>
                <div className="text-xs text-zinc-500">{action.description}</div>
              </div>
              <div className="absolute inset-0 rounded-xl ring-1 ring-inset ring-white/5 group-hover:ring-white/10 transition-all"></div>
            </button>
          ))}
        </motion.div>

        {/* Git Sync Card */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, delay: 0.15 }}
          className="w-full max-w-md mb-8"
        >
          <div className="bg-gradient-to-br from-zinc-900/80 to-zinc-800/30 rounded-xl border border-zinc-800/50 p-4">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <div className={`p-2 rounded-lg ${gitSyncConfigured ? 'bg-green-500/20' : 'bg-zinc-700/50'}`}>
                  <GitBranch className={`w-5 h-5 ${gitSyncConfigured ? 'text-green-400' : 'text-zinc-400'}`} />
                </div>
                <div>
                  <div className="text-sm font-medium text-zinc-200">
                    {gitSyncConfigured ? 'Git Sync Active' : 'Enable Git Sync'}
                  </div>
                  <div className="text-xs text-zinc-500">
                    {gitSyncConfigured ? 'Your notes are synced via git' : 'Sync notes with a git remote'}
                  </div>
                </div>
              </div>
              <button
                onClick={async () => {
                  if (!gitSyncConfigured) {
                    // Open settings for configuration
                    useUiStore.getState().setIsSettingsOpen(true);
                  } else {
                    setIsSyncing(true);
                    try {
                      const result = await invoke<GitSyncResult>('git_sync_now');
                      toast.success(result.message);
                      setLastSyncedAt(new Date());
                      if (result.needs_reload) {
                        const { loadNotes, loadFolders } = useNotesStore.getState();
                        await loadNotes();
                        await loadFolders();
                      }
                    } catch (e) {
                      toast.error(`Sync failed: ${e}`);
                    } finally {
                      setIsSyncing(false);
                    }
                  }
                }}
                disabled={isSyncing}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-all ${gitSyncConfigured
                  ? 'bg-zinc-700 text-zinc-200 hover:bg-zinc-600'
                  : 'bg-blue-600 text-white hover:bg-blue-700'
                  } disabled:opacity-50`}
              >
                {isSyncing ? (
                  <><Loader2 className="w-3 h-3 animate-spin" /> Syncing</>
                ) : gitSyncConfigured ? (
                  <><RefreshCw className="w-3 h-3" /> Sync</>
                ) : (
                  <>Configure</>
                )}
              </button>
            </div>
          </div>
        </motion.div>

        {/* Recent Notes */}
        {recent.length > 0 && (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.5, delay: 0.2 }}
            className="w-full max-w-md"
          >
            <div className="flex items-center gap-2 text-xs text-zinc-500 mb-3 px-1">
              <Clock size={12} />
              <span className="uppercase tracking-wider font-medium">
                Recent Notes
              </span>
            </div>

            <div className="bg-zinc-900/30 rounded-lg border border-zinc-800/50 divide-y divide-zinc-800/50">
              {recent.map((note) => (
                <button
                  key={note.id}
                  onClick={() => useNotesStore.getState().selectNote(note)}
                  className="w-full flex items-center justify-between p-3 hover:bg-zinc-800/30 transition-colors group text-left"
                >
                  <div className="flex items-center gap-3">
                    <div className="p-1.5 rounded-md bg-zinc-800">
                      {note.note_type === "canvas" ? (
                        <Layout size={14} className="text-purple-400" />
                      ) : (
                        <FileText size={14} className="text-blue-400" />
                      )}
                    </div>
                    <div>
                      <div className="text-sm text-zinc-300 group-hover:text-zinc-100 transition-colors truncate max-w-[200px]">
                        {note.title || "Untitled"}
                      </div>
                      <div className="text-xs text-zinc-600">
                        {new Date(note.updated_at).toLocaleDateString()}
                      </div>
                    </div>
                  </div>
                  <ArrowRight
                    size={14}
                    className="text-zinc-600 group-hover:text-zinc-400 transition-colors"
                  />
                </button>
              ))}
            </div>
          </motion.div>
        )}

        {/* Keyboard Shortcut Hint */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.5, delay: 0.4 }}
          className="mt-12 text-center"
        >
          <p className="text-xs text-zinc-600">
            Press{" "}
            <kbd className="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded text-zinc-400 font-mono text-[10px]">
              {modKey}
            </kbd>{" "}
            <kbd className="px-1.5 py-0.5 bg-zinc-800 border border-zinc-700 rounded text-zinc-400 font-mono text-[10px]">
              P
            </kbd>{" "}
            to search notes
          </p>
        </motion.div>
      </div>
    </div>
  );
}
