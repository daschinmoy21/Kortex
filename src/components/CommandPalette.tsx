import { useState, useEffect, useMemo } from 'react';
import useUiStore from '../store/UiStore';
import { useNotesStore } from '../store/notesStore';
import { commandPaletteResults, cycleIndex } from '../lib/note-utils';
import './CommandPalette.css';

export const CommandPalette = () => {
  const { searchQuery, setSearchQuery, searchResults, isCommandPaletteOpen, closeCommandPalette } = useUiStore();
  const { notes, selectNote } = useNotesStore();
  const [selectedIndex, setSelectedIndex] = useState(0);

  const displayResults = useMemo(
    () => commandPaletteResults(searchQuery, notes, searchResults),
    [searchQuery, notes, searchResults],
  );

  useEffect(() => {
    setSelectedIndex(0);
  }, [displayResults]);

  useEffect(() => {
    if (!isCommandPaletteOpen) return;

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeCommandPalette();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isCommandPaletteOpen, closeCommandPalette]);

  if (!isCommandPaletteOpen) {
    return null;
  }

  const focusEditor = () => {
    setTimeout(() => {
      const editorElement = document.querySelector('.bn-editor') as HTMLElement | null;
      if (editorElement) {
        editorElement.focus();
        return;
      }
      const contentEditable = document.querySelector('[contenteditable="true"]') as HTMLElement | null;
      contentEditable?.focus();
    }, 100);
  };

  const openNoteAt = (index: number) => {
    const note = displayResults[index];
    if (!note) return;
    selectNote(note);
    closeCommandPalette();
    focusEditor();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => cycleIndex(prev, 1, displayResults.length));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => cycleIndex(prev, -1, displayResults.length));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      openNoteAt(selectedIndex);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      closeCommandPalette();
    }
  };

  return (
    <div className="command-palette-overlay" onClick={closeCommandPalette} role="presentation">
      <div
        className="command-palette-container"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Search notes"
      >
        <div className="command-palette-content">
          <input
            type="text"
            placeholder={
              displayResults.length && !searchQuery.trim()
                ? 'Search notes or pick a recent note…'
                : 'Search notes (Esc to close)'
            }
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value, notes)}
            onKeyDown={handleKeyDown}
            autoFocus
            aria-label="Search notes"
          />
          <ul className="command-palette-results" role="listbox">
            {displayResults.length === 0 ? (
              <li className="command-palette-empty" role="option" aria-disabled="true">
                {searchQuery.trim() ? 'No matching notes' : 'No notes yet'}
              </li>
            ) : (
              displayResults.map((note, index) => (
                <li
                  key={note.id}
                  role="option"
                  aria-selected={index === selectedIndex}
                  className={index === selectedIndex ? 'selected' : ''}
                  onClick={() => openNoteAt(index)}
                >
                  <span className="truncate">{note.title || 'Untitled'}</span>
                  {!searchQuery.trim() && (
                    <span className="shortcut">recent</span>
                  )}
                </li>
              ))
            )}
          </ul>
        </div>
      </div>
    </div>
  );
};
