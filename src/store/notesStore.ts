import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { Note, Folder } from '../types/Note';

interface NotesState {
  notes: Note[];
  folders: Folder[];
  currentNote: Note | null;
  isLoading: boolean;
  saveTimeout: NodeJS.Timeout | null;
  loadNotes: () => Promise<void>;
  loadFolders: () => Promise<void>;
  selectNote: (note: Note | null) => void;
  createNote: (noteType?: 'text' | 'canvas', folderId?: string) => Promise<Note>;
  createFolder: (name: string, parentId?: string) => Promise<Folder>;
  updateNote: (id: string, updates: Partial<Note>) => Promise<void>;
  updateFolder: (id: string, updates: Partial<Folder>) => Promise<void>;
  deleteNote: (id: string) => Promise<void>;
  deleteFolder: (id: string) => Promise<void>;
  toggleStar: (id: string) => Promise<void>;
  updateCurrentNoteContent: (content: string) => void;
  updateCurrentNoteTitle: (title: string) => void;
  saveCurrentNote: () => Promise<void>;
}

// Auto-save on window blur to ensure changes are saved
if (typeof window !== 'undefined') {
  window.addEventListener('blur', () => {
    // Get the current state and save if there's a pending timeout
    const timeout = useNotesStore.getState().saveTimeout;
    if (timeout) {
      clearTimeout(timeout);
      useNotesStore.getState().saveCurrentNote();
    }
  });
}

export const useNotesStore = create<NotesState>()(
  (set, get) => ({
    notes: [],
    folders: [],
    currentNote: null,
    isLoading: false,
    saveTimeout: null,

    loadNotes: async () => {
      set({ isLoading: true });
      try {
        // Get notes from Tauri backend
        const notes: Note[] = await invoke('get_all_notes');
        set({ notes, isLoading: false });
      } catch (error) {
        console.error('Failed to load notes:', error);
        set({ isLoading: false });
      }
    },

    loadFolders: async () => {
      try {
        const folders: Folder[] = await invoke('get_all_folders');
        set({ folders });
      } catch (error) {
        console.error('Failed to load folders:', error);
      }
    },

    selectNote: async (note) => {
      // Save current note before switching
      const currentState = get();
      if (currentState.saveTimeout) {
        clearTimeout(currentState.saveTimeout);
        await get().saveCurrentNote();
      }
      set({ currentNote: note });
    },

    createNote: async (noteType = 'text', folderId) => {
      try {
        // Create initial content with a Heading 1 "Untitled" in a single backend call
        // (avoids a race between create_note + save_note)
        const initialContent = JSON.stringify([
          {
            id: crypto.randomUUID(),
            type: "heading",
            props: { level: 1 },
            content: "Untitled",
            children: []
          }
        ]);

        const newNote: Note = await invoke('create_note', {
          title: 'Untitled',
          noteType,
          folderId,
          content: initialContent,
        });

        set((state) => ({
          notes: [newNote, ...state.notes],
        }));

        return newNote;
      } catch (error) {
        console.error('Failed to create note:', error);
        throw error;
      }
    },

    createFolder: async (name, parentId) => {
      try {
        const newFolder: Folder = await invoke('create_folder', { name, parentId });

        set((state) => ({
          folders: [...state.folders, newFolder],
        }));

        return newFolder;
      } catch (error) {
        console.error('Failed to create folder:', error);
        throw error;
      }
    },

    updateNote: async (id, updates) => {
      try {
        // Find the note to update
        const state = get();
        const noteToUpdate = state.notes.find(note => note.id === id);
        if (!noteToUpdate) throw new Error('Note not found');

        // Create updated note
        const updatedNote = { ...noteToUpdate, ...updates, updated_at: new Date().toISOString() };

        // Save via Tauri backend
        await invoke('save_note', { note: updatedNote });

        // Update state
        set((state) => ({
          notes: state.notes.map((note) =>
            note.id === id ? updatedNote : note
          ),
          currentNote: state.currentNote?.id === id ? updatedNote : state.currentNote,
        }));
      } catch (error) {
        console.error('Failed to update note:', error);
        throw error;
      }
    },

    updateFolder: async (id, updates) => {
      try {
        const state = get();
        const folderToUpdate = state.folders.find(folder => folder.id === id);
        if (!folderToUpdate) throw new Error('Folder not found');

        const updatedFolder = { ...folderToUpdate, ...updates, updated_at: new Date().toISOString() };

        await invoke('update_folder', { folder: updatedFolder });

        set((state) => ({
          folders: state.folders.map((folder) =>
            folder.id === id ? updatedFolder : folder
          ),
        }));
      } catch (error) {
        console.error('Failed to update folder:', error);
        throw error;
      }
    },

    deleteNote: async (id) => {
      try {
        // Delete via Tauri backend
        await invoke('delete_note', { noteId: id });

        // Update state
        set((state) => ({
          notes: state.notes.filter((note) => note.id !== id),
          currentNote: state.currentNote?.id === id ? null : state.currentNote,
        }));
      } catch (error) {
        console.error('Failed to delete note:', error);
        throw error;
      }
    },

    deleteFolder: async (id) => {
      try {
        await invoke('delete_folder', { folderId: id });

        set((state) => ({
          folders: state.folders.filter((folder) => folder.id !== id),
          // Also remove notes in this folder
          notes: state.notes.filter((note) => note.folder_id !== id),
          currentNote: state.currentNote?.folder_id === id ? null : state.currentNote,
        }));
      } catch (error) {
        console.error('Failed to delete folder:', error);
        throw error;
      }
    },

    toggleStar: async (id) => {
      try {
        const newStarred = await invoke<boolean>('toggle_star_note', { noteId: id });

        set((state) => ({
          notes: state.notes.map((note) =>
            note.id === id ? { ...note, starred: newStarred } : note
          ),
          currentNote: state.currentNote?.id === id
            ? { ...state.currentNote, starred: newStarred }
            : state.currentNote,
        }));
      } catch (error) {
        console.error('Failed to toggle star:', error);
        throw error;
      }
    },

    updateCurrentNoteContent: (content: string) => {
      const state = get();

      if (state.currentNote) {
        // Clear any existing save timeout
        if (state.saveTimeout) {
          clearTimeout(state.saveTimeout);
        }

        // Update the current note content locally
        const updatedNote = { ...state.currentNote, content };

        // Longer debounce for canvas notes (1500ms) vs text notes (600ms)
        const debounceMs = state.currentNote.note_type === 'canvas' ? 1500 : 600;
        const newTimeout = setTimeout(() => {
          get().saveCurrentNote();
        }, debounceMs);

        set((state) => ({
          currentNote: updatedNote,
          notes: state.notes.map((note) =>
            note.id === updatedNote.id ? updatedNote : note
          ),
          saveTimeout: newTimeout,
        }));
      }
    },

    updateCurrentNoteTitle: (title: string) => {
      const state = get();
      if (!state.currentNote) return;

      // Update the current note title locally
      const updatedNote = { ...state.currentNote, title };

      // We do NOT clear existing timeout here because we want the pending content save (if any) to proceed.
      // And we do NOT save immediately; we rely on the autosave loop or the content change trigger.

      set((state) => ({
        currentNote: updatedNote,
        notes: state.notes.map((note) =>
          note.id === updatedNote.id ? updatedNote : note
        ),
      }));

      // We do NOT save immediately here anymore. 
      // The EditorProvider will sync the title change, which comes from content change.
      // The content change triggers the debounced save.
      // If we save here, we might race with the content save.
    },

    saveCurrentNote: async () => {
      const state = get();
      if (state.currentNote) {
        try {
          await invoke('save_note', { note: state.currentNote });
          set({ saveTimeout: null });
        } catch (error) {
          console.error('Failed to save note:', error);
        }
      }
    },
  })
);
