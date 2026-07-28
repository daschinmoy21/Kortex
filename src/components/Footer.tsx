import { footerSaveLabel } from '../lib/note-utils';

interface FooterProps {
  wordCount: number;
  isSaved: boolean;
}

export default function Footer({ wordCount, isSaved }: FooterProps) {
  const saveLabel = footerSaveLabel(isSaved);

  return (
    <footer className="h-8 border-t border-zinc-800 px-6 flex items-center justify-between text-xs text-zinc-400 flex-shrink-0">
      <div className="flex items-center space-x-4">
        <span>Words: {wordCount}</span>
        <span
          className={isSaved ? 'text-zinc-500' : 'text-amber-400/90'}
          aria-live="polite"
          data-testid="footer-save-status"
        >
          {saveLabel}
        </span>
      </div>
    </footer>
  );
}
