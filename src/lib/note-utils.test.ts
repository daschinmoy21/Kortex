import assert from 'node:assert/strict';
import {
  openCreatedNote,
  countWordsInNoteContent,
  recentNotes,
  commandPaletteResults,
  cycleIndex,
} from './note-utils.ts';
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

// Readable text is "Hello world one two three" → 5 words (not JSON keys)
assert.equal(countWordsInNoteContent(blockDoc), 5);
// Pretty-printed JSON still must not count structural tokens as words
const pretty = JSON.stringify(JSON.parse(blockDoc), null, 2);
assert.equal(countWordsInNoteContent(pretty), 5);
assert.ok(
  pretty.split(/\s+/).filter((w) => w.length > 0).length > 5,
  'sanity: naive split of pretty JSON overcounts vs real text',
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

// --- recentNotes / commandPaletteResults / cycleIndex ---
const n1 = makeNote({ id: '1', title: 'Older', updated_at: '2026-01-01T00:00:00.000Z' });
const n2 = makeNote({ id: '2', title: 'Newer', updated_at: '2026-06-01T00:00:00.000Z' });
const n3 = makeNote({ id: '3', title: 'Mid', updated_at: '2026-03-01T00:00:00.000Z' });

assert.deepEqual(
  recentNotes([n1, n2, n3], 2).map((n) => n.id),
  ['2', '3'],
);

assert.deepEqual(
  commandPaletteResults('', [n1, n2], [n1]).map((n) => n.id),
  ['2', '1'],
  'empty query uses recent notes, not fuse matches',
);
assert.deepEqual(
  commandPaletteResults('  x  ', [n1, n2], [n1]).map((n) => n.id),
  ['1'],
  'non-empty query uses fuse matches',
);

assert.equal(cycleIndex(0, 1, 0), 0);
assert.equal(cycleIndex(0, 1, 3), 1);
assert.equal(cycleIndex(2, 1, 3), 0);
assert.equal(cycleIndex(0, -1, 3), 2);

console.log('note-utils palette helpers: ok');
