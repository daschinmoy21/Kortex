import { create } from 'zustand';
import Fuse from 'fuse.js';
import { Note, Folder } from '../types/Note';
import { invoke } from '@tauri-apps/api/core';

interface UiState {
  isSearchActive: boolean;
  searchQuery: string;
  searchResults: Note[];
  isCommandPaletteOpen: boolean;
  toggleSearch: () => void;
  setSearchQuery: (query: string, notes: Note[]) => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  loadApiKey: () => Promise<void>;

  // States moved from Sidebar
  deleteConfirmId: string | null;
  deleteConfirmFolderId: string | null;
  renamingNoteId: string | null;
  renameValue: string;
  contextMenu: { x: number; y: number; note?: Note; folder?: Folder } | null;
  isSettingsOpen: boolean;
  isKanbanOpen: boolean;
  isSupportOpen: boolean;
  isRecording: boolean;
  isProcessingRecording: boolean;
  recordingStartTime: number | null;
  isAiSidebarOpen: boolean;
  isSidebarFloating: boolean;
  expandedFolders: Set<string>;
  googleApiKey: string;
  hasGoogleApiKey: boolean;
  gitSyncConfigured: boolean;
  isSyncing: boolean; // [NEW]
  lastSyncedAt: Date | null; // [NEW]
  editor: any | null;

  // AI Chat State (persists across sidebar toggles)
  aiMessages: Array<{ role: "user" | "assistant"; content: string; actionStatus?: "pending" | "approved" | "refused" }>;
  aiStreamingMessage: string;
  aiIsLoading: boolean;

  // Actions for new states
  setDeleteConfirmId: (id: string | null) => void;
  setDeleteConfirmFolderId: (id: string | null) => void;
  startRenaming: (noteId: string, currentTitle: string) => void;
  finishRenaming: () => void;
  setRenameValue: (value: string) => void;
  setContextMenu: (contextMenu: { x: number; y: number; note?: Note; folder?: Folder } | null) => void;
  setIsSettingsOpen: (isOpen: boolean) => void;
  setIsKanbanOpen: (isOpen: boolean) => void;
  setIsSupportOpen: (isOpen: boolean) => void;
  setIsRecording: (isRecording: boolean) => void;
  setIsProcessingRecording: (isProcessing: boolean) => void;
  setIsAiSidebarOpen: (isOpen: boolean) => void;
  setIsSidebarFloating: (isFloating: boolean) => void;
  setExpandedFolders: (folders: Set<string>) => void;
  setGoogleApiKey: (key: string) => void;
  ensureGoogleApiKey: () => Promise<string>;
  setGitSyncConfigured: (configured: boolean) => void;
  setIsSyncing: (isSyncing: boolean) => void; // [NEW]
  setLastSyncedAt: (date: Date | null) => void; // [NEW]
  setEditor: (editor: any) => void;

  // AI Chat Actions
  addAiMessage: (message: { role: "user" | "assistant"; content: string }) => void;
  updateLastAiMessage: (content: string) => void;
  setAiMessageStatus: (index: number, status: "pending" | "approved" | "refused") => void;
  setAiStreamingMessage: (msg: string) => void;
  setAiIsLoading: (loading: boolean) => void;
  clearAiChat: () => void;
}

const useUiStore = create<UiState>((set) => ({
  isSearchActive: false,
  searchQuery: '',
  searchResults: [],
  isCommandPaletteOpen: false,
  toggleSearch: () => set((state) => ({ isSearchActive: !state.isSearchActive })),
  setSearchQuery: (query, notes) => {
    set({ searchQuery: query });
    if (query) {
      const fuse = new Fuse(notes, {
        keys: ['title', 'content'],
        includeScore: true,
      });
      const results = fuse.search(query);
      set({ searchResults: results.map((result) => result.item) });
    } else {
      set({ searchResults: [] });
    }
  },
  openCommandPalette: () => set({ isCommandPaletteOpen: true }),
  closeCommandPalette: () => set({ isCommandPaletteOpen: false, searchQuery: '', searchResults: [] }),

  // Load API key existence on initialization (never the raw key)
  loadApiKey: async () => {
    try {
      const hasKey = await invoke<boolean>('has_google_api_key');
      set({ hasGoogleApiKey: hasKey });
    } catch (error) {
      console.error('Failed to check API key:', error);
    }
  },

  // States moved from Sidebar
  deleteConfirmId: null,
  renamingNoteId: null,
  renameValue: '',
  contextMenu: null,
  isSettingsOpen: false,
  isKanbanOpen: false,
  isSupportOpen: false,
  isRecording: false,
  isAiSidebarOpen: false,
  isSidebarFloating: false,
  expandedFolders: new Set(),
  deleteConfirmFolderId: null,
  googleApiKey: '',
  hasGoogleApiKey: false,
  gitSyncConfigured: false,
  isSyncing: false, // [NEW]
  lastSyncedAt: null, // [NEW]
  editor: null,

  // AI Chat State
  aiMessages: [],
  aiStreamingMessage: '',
  aiIsLoading: false,

  isProcessingRecording: false,
  recordingStartTime: null,

  // Actions for new states
  setDeleteConfirmId: (id) => set({ deleteConfirmId: id }),
  setDeleteConfirmFolderId: (id) => set({ deleteConfirmFolderId: id }),
  startRenaming: (noteId, currentTitle) => set({ renamingNoteId: noteId, renameValue: currentTitle }),
  finishRenaming: () => set({ renamingNoteId: null, renameValue: '' }),
  setRenameValue: (value) => set({ renameValue: value }),
  setContextMenu: (contextMenu) => set({ contextMenu }),
  setIsSettingsOpen: (isOpen) => set({ isSettingsOpen: isOpen }),
  setIsKanbanOpen: (isOpen) => set({ isKanbanOpen: isOpen }),
  setIsSupportOpen: (isOpen) => set({ isSupportOpen: isOpen }),
  setIsRecording: (isRecording) => set({
    isRecording,
    recordingStartTime: isRecording ? Date.now() : null
  }),
  setIsProcessingRecording: (isProcessing) => set({ isProcessingRecording: isProcessing }),
  setIsAiSidebarOpen: (isOpen) => set({ isAiSidebarOpen: isOpen }),
  setIsSidebarFloating: (isFloating) => set({ isSidebarFloating: isFloating }),
  setExpandedFolders: (expandedFolders) => set({ expandedFolders }),
  setGoogleApiKey: (key) => set({ googleApiKey: key, hasGoogleApiKey: !!key }),

  ensureGoogleApiKey: async (): Promise<string> => {
    const { googleApiKey } = useUiStore.getState();
    if (googleApiKey) return googleApiKey;
    try {
      const key = await invoke<string>('get_google_api_key');
      set({ googleApiKey: key, hasGoogleApiKey: !!key });
      return key;
    } catch {
      return '';
    }
  },
  setGitSyncConfigured: (configured) => set({ gitSyncConfigured: configured }),
  setIsSyncing: (isSyncing) => set({ isSyncing }), // [NEW]
  setLastSyncedAt: (date) => set({ lastSyncedAt: date }), // [NEW]
  setEditor: (editor) => set({ editor }),

  // AI Chat Actions
  addAiMessage: (message) => set((state) => ({
    aiMessages: [...state.aiMessages, { ...message, actionStatus: message.role === "assistant" ? "pending" : undefined }]
  })),
  updateLastAiMessage: (content) => set((state) => {
    const msgs = [...state.aiMessages];
    if (msgs.length > 0 && msgs[msgs.length - 1].role === "assistant") {
      msgs[msgs.length - 1].content = content;
    }
    return { aiMessages: msgs };
  }),
  setAiMessageStatus: (index, status) => set((state) => {
    const msgs = [...state.aiMessages];
    if (msgs[index]) {
      msgs[index].actionStatus = status;
    }
    return { aiMessages: msgs };
  }),
  setAiStreamingMessage: (msg) => set({ aiStreamingMessage: msg }),
  setAiIsLoading: (loading) => set({ aiIsLoading: loading }),
  clearAiChat: () => set({ aiMessages: [], aiStreamingMessage: '', aiIsLoading: false }),
}));

export default useUiStore;
