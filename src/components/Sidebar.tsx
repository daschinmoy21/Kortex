const devLog = (...args: unknown[]) => { if (import.meta.env.DEV) console.log(...args); };

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { Resizable } from "re-resizable";
import {
  Plus,
  ListTodo,
  Search,
  Files,
  Sparkles,
  Trash2,
  ChevronDown,
  SettingsIcon,
  Layout,
  Home,
  GitBranch,
  RotateCw,
  Github,
  Star,
} from "lucide-react";
import {
  Description,
  Dialog,
  DialogPanel,
  DialogTitle,
} from "@headlessui/react";
import { useNotesStore } from "../store/notesStore";
import RecStatus from "./RecStatus";
import useUiStore from "../store/UiStore";
import KanbanBoardContainer from "./KanbanBoard";
import { invoke } from "@tauri-apps/api/core";
import { AnimatedFileTree } from "./AnimatedFileTree";
import toast from "react-hot-toast";
import { processTranscription } from "../lib/aiTranscription";

// Type for trash items from backend
interface TrashItem {
  id: string;
  title: string;  // Title from note or name from folder
  original_type: string;  // "note" or "folder"
  filename: string;
  deleted_at: string;
}

export const Sidebar = () => {
  // Subscribing to state changes individually
  const currentNote = useNotesStore((state) => state.currentNote);
  const notes = useNotesStore((state) => state.notes);

  // Getting actions (they don't cause re-renders)
  const {
    loadNotes,
    loadFolders,
    selectNote,
    deleteNote,
    createNote,
    createFolder,
    updateNote,
    updateFolder,
    deleteFolder,
    toggleStar,
    updateCurrentNoteContent,
  } = useNotesStore.getState();

  // Get starred notes
  const starredNotes = notes.filter(note => note.starred);

  // Subscribing to UI state changes individually
  const deleteConfirmId = useUiStore((state) => state.deleteConfirmId);
  const deleteConfirmFolderId = useUiStore(
    (state) => state.deleteConfirmFolderId,
  );
  const renamingNoteId = useUiStore((state) => state.renamingNoteId);
  const renameValue = useUiStore((state) => state.renameValue);
  const contextMenu = useUiStore((state) => state.contextMenu);
  const isKanbanOpen = useUiStore((state) => state.isKanbanOpen);
  const isRecording = useUiStore((state) => state.isRecording);
  const isProcessingRecording = useUiStore(
    (state) => state.isProcessingRecording,
  );
  const gitSyncConfigured = useUiStore((state) => state.gitSyncConfigured);
  const isSyncing = useUiStore((state) => state.isSyncing);

  // Folder renaming state
  const [renamingFolderId, setRenamingFolderId] = useState<string | null>(null);
  const [folderRenameValue, setFolderRenameValue] = useState("");
  const [selectedFolderId, setSelectedFolderId] = useState<string | null>(null);
  const expandedFolders = useUiStore((state) => state.expandedFolders);

  // Trash dialog state
  const [isTrashOpen, setIsTrashOpen] = useState(false);
  const [trashItems, setTrashItems] = useState<TrashItem[]>([]);
  const [isLoadingTrash, setIsLoadingTrash] = useState(false);

  // Getting UI actions
  const {
    openCommandPalette,
    setDeleteConfirmId,
    setDeleteConfirmFolderId,
    startRenaming,
    finishRenaming,
    setRenameValue,
    setContextMenu,
    setIsKanbanOpen,
    setIsRecording,
    setIsSettingsOpen,
    setExpandedFolders,
    setIsProcessingRecording,
  } = useUiStore.getState();

  useEffect(() => {
    loadNotes();
    loadFolders();
  }, [loadNotes, loadFolders]);

  useEffect(() => {
    const handleClickOutside = () => setContextMenu(null);
    window.addEventListener("click", handleClickOutside);
    return () => window.removeEventListener("click", handleClickOutside);
  }, [setContextMenu]);

  const handleCreateNote = async (
    noteType: "text" | "canvas" = "text",
    folderId?: string,
  ) => {
    try {
      await createNote(noteType, folderId || selectedFolderId || undefined);
    } catch (error) {
      console.error("Error creating note", error);
    }
  };

  const handleCreateFolder = async (folderId?: string) => {
    try {
      await createFolder(
        "New Folder",
        folderId || selectedFolderId || undefined,
      );
    } catch (error) {
      console.error("Error creating folder", error);
    }
  };

  const handleDeleteFolder = async (folderId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setDeleteConfirmFolderId(folderId);
  };

  const handleDeleteNote = async (noteId: string, e: React.MouseEvent) => {
    e.stopPropagation(); //to stop event bubbling
    setDeleteConfirmId(noteId);
  };

  const confirmDeleteFolder = async () => {
    if (deleteConfirmFolderId) {
      try {
        await deleteFolder(deleteConfirmFolderId);
        setDeleteConfirmFolderId(null);
      } catch (error) {
        console.error("Failed to delete folder", error);
      }
    }
  };

  const confirmDelete = async () => {
    if (deleteConfirmId) {
      try {
        await deleteNote(deleteConfirmId);
        setDeleteConfirmId(null);
      } catch (error) {
        console.error("Failed to delete note", error);
      }
    }
  };

  const handleRename = () => {
    if (renamingNoteId && renameValue.trim()) {
      updateNote(renamingNoteId, { title: renameValue.trim() });
    }
    finishRenaming();
  };

  const handleFolderRename = () => {
    if (renamingFolderId && folderRenameValue.trim()) {
      updateFolder(renamingFolderId, { name: folderRenameValue.trim() });
    }
    setRenamingFolderId(null);
    setFolderRenameValue("");
  };

  const selectFolder = (folderId: string) => {
    setSelectedFolderId(folderId);
  };

  // Fetch trash items
  const loadTrashItems = async () => {
    setIsLoadingTrash(true);
    try {
      const items = await invoke<TrashItem[]>('get_trash_items');
      setTrashItems(items);
    } catch (error) {
      console.error('Failed to load trash:', error);
      toast.error('Failed to load trash items');
    } finally {
      setIsLoadingTrash(false);
    }
  };

  // Restore item from trash
  const handleRestore = async (item: TrashItem) => {
    try {
      await invoke('restore_from_trash', { itemId: item.id, itemType: item.original_type });
      toast.success(item.original_type === 'note' ? 'Note restored!' : 'Folder and contents restored!');
      // Refresh both trash list and notes/folders
      await loadTrashItems();
      loadNotes();
      loadFolders();
    } catch (error) {
      console.error('Failed to restore:', error);
      toast.error('Failed to restore item');
    }
  };

  // Empty all trash
  const handleEmptyTrash = async () => {
    try {
      const count = await invoke<number>('empty_trash');
      toast.success(`Permanently deleted ${count} items`);
      setTrashItems([]);
    } catch (error) {
      console.error('Failed to empty trash:', error);
      toast.error('Failed to empty trash');
    }
  };

  // Open trash dialog and load items
  const openTrash = () => {
    setIsTrashOpen(true);
    loadTrashItems();
  };

  return (
    <>
      <Resizable
        as="aside"
        className="bg-zinc-950 pb-2 border-r border-zinc-900 flex flex-col h-full text-zinc-400 select-none"
        defaultSize={{ width: 260, height: "100%" }}
        minWidth={240}
        maxWidth={360}
        enable={{ right: true }}
        handleClasses={{
          right:
            "w-0.5 bg-zinc-900 hover:bg-blue-500/50 transition-colors delay-150",
        }}
      >
        {/* === Scrollable Content Area === */}
        <div className="flex flex-col flex-1 overflow-hidden px-3 py-2 gap-4">
          {/* 1. Primary Navigation */}
          <div className="space-y-0.5">
            <button
              onClick={openCommandPalette}
              className="w-full border border-blue-200/40 flex items-center gap-3 px-2 py-1.5 mb-2 text-sm font-medium hover:bg-zinc-900 text-zinc-400 hover:text-zinc-200 rounded-md transition-colors group"
            >
              <Search size={16} />
              <span>Search</span>
              <kbd className="ml-auto pointer-events-none hidden h-5 select-none items-center gap-1 rounded border border-zinc-800 bg-zinc-900 px-1.5 font-mono text-[10px] text-zinc-500 opacity-0 group-hover:opacity-100 font-medium transition-opacity sm:flex">
                <span className="text-xs">⌘</span>P
              </kbd>
            </button>
            <button
              onClick={() => {
                selectNote(null);
                setSelectedFolderId(null);
              }}
              className="w-full flex items-center gap-3 px-2 py-1.5 mb-2 text-sm font-medium hover:bg-zinc-900 text-zinc-400 hover:text-zinc-200 rounded-md transition-colors"
            >
              <Home size={16} />
              <span>Home</span>
            </button>
            <button
              onClick={() => setIsKanbanOpen(true)}
              className="w-full flex items-center gap-3 px-2 py-1.5 text-sm font-medium hover:bg-zinc-900 text-zinc-400 hover:text-zinc-200 rounded-md transition-colors"
            >
              <ListTodo size={16} />
              <span>To-do List</span>
            </button>
            <button
              disabled={!currentNote || currentNote.note_type === "canvas"}
              onClick={async () => {
                if (!currentNote || currentNote.note_type === "canvas") return;
                if (!isRecording) {
                  try {
                    await invoke("start_recording");
                    setIsRecording(true);
                  } catch (error) {
                    console.error("Recording start failed:", error);
                    toast.error(`Recording failed: ${error}`);
                  }
                }
              }}
              className={`w-full flex items-center gap-3 px-2 py-1.5 text-sm font-medium hover:bg-zinc-900 rounded-md transition-colors
                     ${isRecording ? "text-red-400 animate-pulse" : "text-zinc-400 hover:text-zinc-200"}
                     ${!currentNote || currentNote.note_type === "canvas" ? "opacity-50 cursor-not-allowed" : ""}
                    `}
            >
              <Sparkles size={16} />
              <span>AI Audio Capture</span>
              {isRecording && (
                <span className="ml-auto w-2 h-2 rounded-full bg-red-500"></span>
              )}
            </button>

            <button
              onClick={() => {
                devLog("Opening Settings via Sidebar");
                setIsSettingsOpen(true);
              }}
              className="w-full flex items-center gap-3 px-2 py-1.5 text-sm font-medium hover:bg-zinc-900 text-zinc-400 hover:text-zinc-200 rounded-md transition-colors"
            >
              <SettingsIcon size={16} />
              <span>Settings</span>
            </button>
          </div>

          {/* Favorites Section (only show if there are starred notes) */}
          {starredNotes.length > 0 && (
            <div className="space-y-1">
              <div className="flex items-center gap-1 px-2 text-xs font-semibold text-yellow-500/80">
                <Star size={12} className="fill-yellow-500" />
                <span>Favorites</span>
              </div>
              <div className="max-h-32 overflow-y-auto scrollbar-thin scrollbar-thumb-zinc-800 scrollbar-track-transparent space-y-0.5 px-1">
                {starredNotes.map((note) => (
                  <button
                    key={note.id}
                    onClick={() => selectNote(note)}
                    className={`w-full flex items-center gap-2 px-2 py-1.5 text-sm rounded-md transition-colors text-left ${currentNote?.id === note.id
                      ? 'bg-zinc-800 text-zinc-200'
                      : 'text-zinc-400 hover:bg-zinc-900 hover:text-zinc-200'
                      }`}
                  >
                    <Star size={12} className="text-yellow-500 fill-yellow-500 flex-shrink-0" />
                    <span className="truncate">{note.title || 'Untitled'}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="flex flex-col flex-1 min-h-0 space-y-1">
            <div className="flex items-center justify-between px-2 text-xs font-semibold text-zinc-500 group cursor-pointer hover:text-zinc-400 transition-colors mb-1">
              <span className="flex items-center gap-1">
                Notes <ChevronDown size={10} />
              </span>
              <div className="flex items-center gap-1 opacity-100 md:opacity-0 group-hover:opacity-100 transition-opacity">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCreateNote("text");
                  }}
                  className="hover:bg-zinc-800 p-0.5 rounded text-zinc-500 hover:text-zinc-300"
                  title="New Note"
                >
                  <Files size={16} />
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCreateNote("canvas");
                  }}
                  className="hover:bg-zinc-800 p-0.5 rounded text-zinc-500 hover:text-zinc-300"
                  title="New Canvas"
                >
                  <Layout size={16} />
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleCreateFolder();
                  }}
                  className="hover:bg-zinc-800 p-0.5 rounded text-zinc-500 hover:text-zinc-300"
                  title="New Folder"
                >
                  <Plus size={20} />
                </button>
              </div>
            </div>

            {/* File Tree Component */}
            <div
              onClick={() => setSelectedFolderId(null)}
              className="flex-1 overflow-y-auto overflow-x-hidden scrollbar-thin scrollbar-thumb-zinc-800 scrollbar-track-transparent"
            >
              <AnimatedFileTree
                onSelectNote={selectNote}
                onSelectFolder={selectFolder}
                onContextMenu={(e, item) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setContextMenu({ x: e.clientX, y: e.clientY, ...item });
                }}
                onDeleteFolder={handleDeleteFolder}
                onDeleteNote={handleDeleteNote}
                selectedFolderId={selectedFolderId}
                selectedNoteId={currentNote?.id || null}
                expandedFolders={expandedFolders}
                onExpandedFoldersChange={setExpandedFolders}
                renamingNoteId={renamingNoteId}
                renamingFolderId={renamingFolderId}
                renameValue={renameValue}
                folderRenameValue={folderRenameValue}
                onRename={handleRename}
                onFolderRename={handleFolderRename}
                onRenameValueChange={setRenameValue}
                onFolderRenameValueChange={setFolderRenameValue}
                onStartRenaming={startRenaming}
                onStartFolderRenaming={(folderId, name) => {
                  setRenamingFolderId(folderId);
                  setFolderRenameValue(name);
                }}
              />
            </div>
          </div>

          {/* Git Sync Status (Visible only if configured) */}
          {gitSyncConfigured && (
            <div className="px-3 py-2.5 mx-2 mb-2 rounded-lg bg-gradient-to-r from-zinc-800/50 to-emerald-900/20 border border-zinc-800/50">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  {isSyncing ? (
                    <>
                      <RotateCw className="w-3.5 h-3.5 animate-spin text-zinc-400" />
                      <span className="text-xs font-medium bg-gradient-to-r from-zinc-400 to-zinc-600 bg-clip-text text-transparent">Syncing...</span>
                    </>
                  ) : (
                    <>
                      <GitBranch className="w-3.5 h-3.5 text-emerald-500" />
                      <span className="text-xs font-medium bg-gradient-to-r from-emerald-400 to-emerald-600 bg-clip-text text-transparent">Git Sync</span>
                    </>
                  )}
                </div>
                {!isSyncing && (
                  <span className="text-[10px] text-zinc-500">
                    {useUiStore.getState().lastSyncedAt
                      ? new Date(useUiStore.getState().lastSyncedAt!).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
                      : ''}
                  </span>
                )}
              </div>
            </div>
          )}

          {/* Footer */}
          <div className="p-4 border-t border-zinc-900/50 space-y-1 flex-shrink-0">
            <div className="px-2 text-xs font-semibold text-zinc-600 mb-2">
              Others
            </div>
            <button
              onClick={openTrash}
              className="w-full flex items-center gap-3 px-2 py-1.5 text-sm font-medium hover:bg-zinc-900 text-zinc-500 hover:text-zinc-300 rounded-md transition-colors"
            >
              <Trash2 size={16} />
              <span>Trash</span>
            </button>
            <button
              onClick={async () => {
                try {
                  const { openUrl } = await import('@tauri-apps/plugin-opener');
                  await openUrl('https://github.com/daschinmoy21/Logia');
                } catch (e) {
                  console.error('Failed to open URL:', e);
                }
              }}
              className="w-full flex items-center gap-3 px-2 py-1.5 text-sm font-medium hover:bg-zinc-900 text-zinc-500 hover:text-zinc-300 rounded-md transition-colors"
            >
              <Github size={16} />
              <span>Star on GitHub</span>
            </button>
          </div>
        </div>

        {/* Recording Status Animation (Slide Up/Down) */}
        <AnimatePresence>
          {(isRecording || isProcessingRecording) && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.3, ease: "easeInOut" }}
              className="overflow-hidden border-t border-zinc-800 bg-zinc-900/30"
            >
              <RecStatus
                isRecording={isRecording}
                isProcessing={isProcessingRecording}
                onStop={async () => {
                  setIsProcessingRecording(true);
                  setIsRecording(false); // Stop the recording UI immediately

                  try {
                    // 1. Stop recording and get transcript
                    devLog("[Logia] Stopping recording...");
                    let transcriptionResult: string;
                    try {
                      transcriptionResult = await invoke<string>("stop_recording");
                      devLog("[Logia] stop_recording returned:", transcriptionResult);
                    } catch (stopErr) {
                      console.error("[Logia] stop_recording invoke failed:", stopErr);
                      throw new Error(`Stop recording failed: ${stopErr}`);
                    }

                    let transcriptText = "";

                    try {
                      const parsed = JSON.parse(transcriptionResult);
                      transcriptText = parsed.text || transcriptionResult;
                      devLog("[Logia] Parsed transcript text:", transcriptText?.substring(0, 100));
                    } catch {
                      transcriptText = transcriptionResult;
                      devLog("[Logia] Using raw transcript:", transcriptText?.substring(0, 100));
                    }

                    if (!transcriptText || transcriptText.trim() === "") {
                      throw new Error("No transcript generated - empty result");
                    }

                    // 2. Ensure we have a note
                    let noteToUpdate = currentNote;
                    if (!noteToUpdate) {
                      try {
                        await createNote("text");
                        // Wait a tick for store update or fetch freshly
                        noteToUpdate = useNotesStore.getState().currentNote;
                      } catch (e) {
                        console.error("Failed to create note", e);
                        toast.error("Could not create new note");
                        setIsProcessingRecording(false);
                        return;
                      }
                    }

                    if (!noteToUpdate) {
                      toast.error("No active note found.");
                      setIsProcessingRecording(false);
                      return;
                    }

                    // 3. Process with AI Utility
                    const googleApiKey = useUiStore.getState().googleApiKey;
                    const editor = useUiStore.getState().editor; // Access editor instance
                    const currentContent =
                      JSON.parse(noteToUpdate.content || "[]") || [];

                    await processTranscription({
                      transcriptionText: transcriptText,
                      googleApiKey,
                      editor,
                      updateCurrentNoteContent,
                      saveCurrentNote: useNotesStore.getState().saveCurrentNote,
                      currentContent,
                    });
                  } catch (e) {
                    console.error("Audio capture process failed", e);
                    const errorMsg = e instanceof Error ? e.message : String(e);
                    toast.error(`Recording process failed: ${errorMsg}`);
                  } finally {
                    setIsProcessingRecording(false);
                  }
                }}
              />
            </motion.div>
          )}
        </AnimatePresence>
      </Resizable>

      {/* Delete confirmation dialog */}
      <Dialog
        open={deleteConfirmId !== null}
        onClose={() => setDeleteConfirmId(null)}
        className="relative z-[1000]"
      >
        <div
          className="fixed inset-0 bg-black/30 backdrop-blur-sm"
          onClick={() => setDeleteConfirmId(null)}
        />
        <div className="fixed inset-0 flex items-center justify-center p-4">
          <DialogPanel className="bg-zinc-800 border border-zinc-700 rounded-xl p-6 max-w-sm w-full">
            <DialogTitle className="text-white font-medium mb-2">
              Delete Note
            </DialogTitle>
            <Description className="text-zinc-400 text-sm mb-6">
              This will move the note to Trash. You can restore it later.
            </Description>
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setDeleteConfirmId(null)}
                className="px-4 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-700 rounded-lg"
              >
                Cancel
              </button>
              <button
                onClick={confirmDelete}
                className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg"
              >
                Move to Trash
              </button>
            </div>
          </DialogPanel>
        </div>
      </Dialog>

      {/* Delete folder confirmation dialog */}
      <Dialog
        open={deleteConfirmFolderId !== null}
        onClose={() => setDeleteConfirmFolderId(null)}
        className="relative z-[1000]"
      >
        <div
          className="fixed inset-0 bg-black/30 backdrop-blur-sm"
          onClick={() => setDeleteConfirmFolderId(null)}
        />
        <div className="fixed inset-0 flex items-center justify-center p-4">
          <DialogPanel className="bg-zinc-800 border border-zinc-700 rounded-xl p-6 max-w-sm w-full">
            <DialogTitle className="text-white font-medium mb-2">
              Delete Folder
            </DialogTitle>
            <Description className="text-zinc-400 text-sm mb-6">
              This will move the folder to Trash. You can restore it later.
            </Description>
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => setDeleteConfirmFolderId(null)}
                className="px-4 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-700 rounded-lg"
              >
                Cancel
              </button>
              <button
                onClick={confirmDeleteFolder}
                className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg"
              >
                Move to Trash
              </button>
            </div>
          </DialogPanel>
        </div>
      </Dialog>

      {/* Trash Dialog */}
      <Dialog
        open={isTrashOpen}
        onClose={() => setIsTrashOpen(false)}
        className="relative z-[1000]"
      >
        <div
          className="fixed inset-0 bg-black/30 backdrop-blur-sm"
          onClick={() => setIsTrashOpen(false)}
        />
        <div className="fixed inset-0 flex items-center justify-center p-4">
          <DialogPanel className="bg-zinc-900 border border-zinc-800 rounded-xl p-6 max-w-md w-full max-h-[70vh] flex flex-col">
            <DialogTitle className="text-white font-medium mb-1 flex items-center gap-2">
              <Trash2 size={18} />
              Trash
            </DialogTitle>
            <Description className="text-zinc-500 text-sm mb-4">
              Items are permanently deleted after 14 days.
            </Description>

            {/* Trash items list */}
            <div className="flex-1 overflow-y-auto scrollbar-thin scrollbar-thumb-zinc-700 scrollbar-track-transparent space-y-2 min-h-[100px] max-h-[300px]">
              {isLoadingTrash ? (
                <div className="flex items-center justify-center py-8">
                  <RotateCw className="w-5 h-5 animate-spin text-zinc-500" />
                </div>
              ) : trashItems.length === 0 ? (
                <div className="text-center py-8 text-zinc-500 text-sm">
                  Trash is empty
                </div>
              ) : (
                trashItems.map((item) => (
                  <div
                    key={item.id}
                    className="flex items-center justify-between p-3 bg-zinc-800/50 rounded-lg hover:bg-zinc-800 transition-colors"
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className={`text-xs px-1.5 py-0.5 rounded ${item.original_type === 'note'
                          ? 'bg-blue-900/50 text-blue-400'
                          : 'bg-amber-900/50 text-amber-400'
                          }`}>
                          {item.original_type}
                        </span>
                        <span className="text-sm text-zinc-300 truncate">
                          {item.title}
                        </span>
                      </div>
                      <div className="text-xs text-zinc-500 mt-1">
                        Deleted {new Date(item.deleted_at).toLocaleDateString()}
                      </div>
                    </div>
                    <button
                      onClick={() => handleRestore(item)}
                      className="ml-2 px-3 py-1.5 text-xs bg-zinc-700 hover:bg-zinc-600 text-zinc-200 rounded-md transition-colors"
                    >
                      Restore
                    </button>
                  </div>
                ))
              )}
            </div>

            {/* Footer actions */}
            <div className="flex gap-3 justify-between mt-4 pt-4 border-t border-zinc-800">
              <button
                onClick={handleEmptyTrash}
                disabled={trashItems.length === 0}
                className="px-4 py-2 text-sm text-red-400 hover:text-red-300 hover:bg-zinc-800 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
              >
                Empty Trash
              </button>
              <button
                onClick={() => setIsTrashOpen(false)}
                className="px-4 py-2 text-sm text-zinc-400 hover:text-white hover:bg-zinc-800 rounded-lg transition-colors"
              >
                Close
              </button>
            </div>
          </DialogPanel>
        </div>
      </Dialog>

      <AnimatePresence>
        {contextMenu && (
          <motion.div
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={{ duration: 0.1, ease: "easeOut" }}
            style={{ top: contextMenu.y, left: contextMenu.x }}
            className="absolute z-50 bg-zinc-900 border border-zinc-800 rounded-lg shadow-xl py-1.5 min-w-[140px] overflow-hidden"
          >
            {contextMenu.note && (
              <>
                <button
                  onClick={() => {
                    toggleStar(contextMenu!.note!.id);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-yellow-400 hover:bg-zinc-800 hover:text-yellow-300 transition-colors flex items-center gap-2"
                >
                  <Star size={12} className={contextMenu.note.starred ? "fill-yellow-400" : ""} />
                  {contextMenu.note.starred ? 'Unstar' : 'Star'}
                </button>
                <button
                  onClick={() => {
                    startRenaming(
                      contextMenu!.note!.id,
                      contextMenu!.note!.title || "Untitled",
                    );
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
                >
                  Rename
                </button>
                <button
                  onClick={(e) => {
                    handleDeleteNote(contextMenu!.note!.id, e);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-red-400 hover:bg-zinc-800 hover:text-red-300 transition-colors"
                >
                  Delete
                </button>
              </>
            )}
            {contextMenu.folder && (
              <>
                <button
                  onClick={() => {
                    handleCreateNote("text", contextMenu!.folder!.id);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
                >
                  New Note
                </button>
                <button
                  onClick={() => {
                    handleCreateNote("canvas", contextMenu!.folder!.id);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
                >
                  New Canvas
                </button>
                <button
                  onClick={() => {
                    handleCreateFolder(contextMenu!.folder!.id);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
                >
                  New Folder
                </button>
                <div className="h-px bg-zinc-800 my-1 mx-2" />
                <button
                  onClick={() => {
                    setRenamingFolderId(contextMenu!.folder!.id);
                    setFolderRenameValue(contextMenu!.folder!.name);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 hover:text-zinc-100 transition-colors"
                >
                  Rename
                </button>
                <button
                  onClick={(e) => {
                    handleDeleteFolder(contextMenu!.folder!.id, e);
                    setContextMenu(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-red-400 hover:bg-zinc-800 hover:text-red-300 transition-colors"
                >
                  Delete
                </button>
              </>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <KanbanBoardContainer
        isOpen={isKanbanOpen}
        onClose={() => setIsKanbanOpen(false)}
      />
    </>
  );
};
