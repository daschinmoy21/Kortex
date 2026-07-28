import { AnimatePresence, motion } from 'framer-motion';
import { footerSaveLabel } from '../lib/note-utils';
import { prefersReducedMotion } from '../lib/utils';

interface FooterProps {
  wordCount: number;
  isSaved: boolean;
}

export default function Footer({ wordCount, isSaved }: FooterProps) {
  const saveLabel = footerSaveLabel(isSaved);
  const reduceMotion = prefersReducedMotion();

  return (
    <footer className="h-8 border-t border-zinc-800 px-6 flex items-center justify-between text-xs text-zinc-400 flex-shrink-0">
      <div className="flex items-center space-x-4">
        <span>Words: {wordCount}</span>
        <span
          className="inline-flex items-center min-w-[4.5rem]"
          aria-live="polite"
          data-testid="footer-save-status"
        >
          <AnimatePresence mode="wait" initial={false}>
            <motion.span
              key={saveLabel}
              initial={reduceMotion ? false : { opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={reduceMotion ? undefined : { opacity: 0, y: -4 }}
              transition={{ duration: 0.15 }}
              className={isSaved ? 'text-zinc-500' : 'text-amber-400/90'}
            >
              {saveLabel}
            </motion.span>
          </AnimatePresence>
        </span>
      </div>
    </footer>
  );
}
