import assert from 'node:assert/strict';
import { openCreatedNote, countWordsInNoteContent } from './note-utils.ts';
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

// --- countWordsInNoteContent ---
const blockDoc = JSON.stringify([
  {
    id: '1',
    type: 'heading',
    props: { level: 1 },
    content: [{ type: 'text', text: 'Hello world', styles: {} }],
    children: [],
  },
  {
    id: '2',
    type: 'paragraph',
    content: [{ type: 'text', text: 'one two three', styles: {} }],
    children: [],
  },
]);

// Naive split on raw JSON would count many "words"; real text has 5
assert.equal(countWordsInNoteContent(blockDoc), 5);
assert.ok(
  blockDoc.split(/\s+/).filter((w) => w.length > 0).length > 5,
  'sanity: raw JSON split overcounts vs real text',
);
assert.equal(countWordsInNoteContent('plain two words'), 3);
assert.equal(countWordsInNoteContent(''), 0);
assert.equal(countWordsInNoteContent(null), 0);

// Nested children
const nested = JSON.stringify([
  {
    type: 'bulletListItem',
    content: [{ type: 'text', text: 'parent item', styles: {} }],
    children: [
      {
        type: 'bulletListItem',
        content: [{ type: 'text', text: 'child words here', styles: {} }],
        children: [],
      },
    ],
  },
]);
assert.equal(countWordsInNoteContent(nested), 5);

console.log('note-utils countWordsInNoteContent: ok');
