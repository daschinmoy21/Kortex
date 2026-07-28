import assert from 'node:assert/strict';
import { openCreatedNote } from './note-utils.ts';
import type { Note } from '../types/Note.ts';

function makeNote(partial: Partial<Note> & { id: string; title: string }): Note {
  return {
    content: '[]',
    note_type: 'text',
    starred: false,
    created_at: '2026-01-01T00:00:00.000Z',
    updated_at: '2026-01-01T00:00:00.000Z',
    ...partial,
  };
}

const existing = makeNote({ id: 'a', title: 'Old' });
const created = makeNote({ id: 'b', title: 'Untitled' });

const opened = openCreatedNote([existing], created);

assert.equal(opened.currentNote.id, 'b');
assert.equal(opened.currentNote.title, 'Untitled');
assert.equal(opened.saveTimeout, null);
assert.deepEqual(
  opened.notes.map((n) => n.id),
  ['b', 'a'],
);

// Idempotent prepend if same id already present
const again = openCreatedNote([created, existing], created);
assert.deepEqual(
  again.notes.map((n) => n.id),
  ['b', 'a'],
);
assert.equal(again.currentNote.id, 'b');

console.log('note-utils openCreatedNote: ok');
