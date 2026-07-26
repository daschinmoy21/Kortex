const devLog = (...args: unknown[]) => { if (import.meta.env.DEV) console.log(...args); };

import { useCallback, useRef } from 'react';
import { Tldraw, setUserPreferences } from 'tldraw';
import 'tldraw/tldraw.css';
import { useNotesStore } from '../store/notesStore';

function Canvas() {
  const { currentNote, updateCurrentNoteContent } = useNotesStore();
  const saveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleMount = useCallback((editor: any) => {
    devLog('Tldraw editor mounted');

    // Set dark mode
    setUserPreferences({ id: 'logia-user', colorScheme: 'dark' });
    // Load initial data from backend if available
    if (currentNote?.content) {
      try {
        const snapshot = JSON.parse(currentNote.content);
        editor.loadSnapshot(snapshot);
        devLog('Loaded snapshot from backend');
      } catch (e) {
        console.error('Failed to parse backend content', e);
      }
    }

    // Listen for changes
    const unsubscribe = editor.store.listen(
      () => {
        // Clear existing timeout
        if (saveTimeoutRef.current) {
          clearTimeout(saveTimeoutRef.current);
        }

        // Debounce save to store
        saveTimeoutRef.current = setTimeout(() => {
          const snapshot = editor.store.getSnapshot();
          const serialized = JSON.stringify(snapshot);
          updateCurrentNoteContent(serialized);
        }, 500);
      },
      { source: 'user' }
    );

    return unsubscribe;
  }, [currentNote?.id]);

  if (!currentNote) return null;

  return (
    <div style={{ height: "100%" }}>
      <Tldraw
        persistenceKey={`note-${currentNote.id}`}
        onMount={handleMount}
      />
    </div>
  );
}

export default Canvas;
