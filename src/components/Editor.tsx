import { Suspense, lazy } from 'react';
import "@blocknote/core/fonts/inter.css";
import "@blocknote/mantine/style.css";
import { useNotesStore } from "../store/notesStore";
import { NoteEditor } from "./NoteEditor";
import { EditorProvider } from "./EditorProvider";
import { EmptyState } from "./EmptyState";
import './styles.css';
import useUiStore from '../store/UiStore';

// Lazy load Canvas to prevent huge bundle load on startup
const Canvas = lazy(() => import("./Canvas"));

function Editor() {
  const {
    currentNote,
    updateCurrentNoteContent,
    updateCurrentNoteTitle,
  } = useNotesStore();
  const { hasGoogleApiKey } = useUiStore();

  // Show empty state when no note is selected
  if (!currentNote) {
    return <EmptyState />;
  }

  // Conditional rendering based on note_type
  if (currentNote.note_type === 'canvas') {
    return (
      <div className="h-full w-full">
        <Suspense fallback={
          <div className="h-full w-full flex items-center justify-center bg-zinc-950">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
          </div>
        }>
          <Canvas />
        </Suspense>
      </div>
    );
  }

  // Key based on API key + note ID ensures the editor remounts when:
  // 1. The user switches notes
  // 2. The user adds/removes/changes the API key (toggling AI features)
  const editorKey = `${currentNote.id}-${hasGoogleApiKey ? 'ai' : 'no-ai'}`;

  return (
    <EditorProvider
      key={editorKey}
      currentNote={currentNote}
      updateCurrentNoteContent={updateCurrentNoteContent}
      updateCurrentNoteTitle={updateCurrentNoteTitle}
    >
      <NoteEditor />
    </EditorProvider>
  );
}

export default Editor;