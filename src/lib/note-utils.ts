import type { Note } from '../types/Note';

/** Merge a newly created note into the list and open it as current. */
export function openCreatedNote(
  notes: Note[],
  newNote: Note,
): { notes: Note[]; currentNote: Note; saveTimeout: null } {
  return {
    notes: [newNote, ...notes.filter((n) => n.id !== newNote.id)],
    currentNote: newNote,
    saveTimeout: null,
  };
}
