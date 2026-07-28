import Editor from "./components/Editor.tsx";
import Header from "./components/Header.tsx";
import "./App.css";
import { Sidebar } from "./components/Sidebar.tsx";
import { useEffect, useRef, useState } from "react";
import useUiStore from "./store/UiStore.ts";
import { CommandPalette } from "./components/CommandPalette.tsx";
import Footer from "./components/Footer.tsx";
import AiSidebar from "./components/AiSidebar.tsx";
import { Settings } from "./components/Settings.tsx";

import { useNotesStore } from "./store/notesStore";
import { Toaster } from "react-hot-toast";
import PreflightModal from "./components/PrereflightModal.tsx";
import { countWordsInNoteContent } from "./lib/note-utils";
import { prefersReducedMotion } from "./lib/utils";

import { AnimatePresence, motion } from "framer-motion";

function App() {
  const {
    openCommandPalette,
    isAiSidebarOpen,
    setIsAiSidebarOpen,
    isSidebarFloating,
    loadApiKey,
  } = useUiStore();
  const { currentNote, saveTimeout } = useNotesStore();

  const wordCount = currentNote
    ? countWordsInNoteContent(currentNote.content)
    : 0;

  const isSaved = !saveTimeout;

  const [showFloatingSidebar, setShowFloatingSidebar] = useState(false);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reduceMotion = prefersReducedMotion();

  // Reset float overlay when pinning sidebar again
  useEffect(() => {
    if (!isSidebarFloating) {
      setShowFloatingSidebar(false);
      if (leaveTimerRef.current) {
        clearTimeout(leaveTimerRef.current);
        leaveTimerRef.current = null;
      }
    }
  }, [isSidebarFloating]);

  useEffect(() => {
    loadApiKey();

    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.altKey) && event.key.toLowerCase() === "p") {
        event.preventDefault();
        openCommandPalette();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [openCommandPalette, loadApiKey]);

  const openFloating = () => {
    if (leaveTimerRef.current) {
      clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
    setShowFloatingSidebar(true);
  };

  const scheduleCloseFloating = () => {
    if (leaveTimerRef.current) clearTimeout(leaveTimerRef.current);
    leaveTimerRef.current = setTimeout(() => {
      setShowFloatingSidebar(false);
      leaveTimerRef.current = null;
    }, 180);
  };

  const spring = reduceMotion
    ? { duration: 0.01 }
    : { type: "spring" as const, stiffness: 320, damping: 32 };

  return (
    <div className="bg-zinc-950 flex flex-col h-screen overflow-hidden">
      <PreflightModal />
      <Settings />
      <CommandPalette />
      <Header />

      <div className="flex flex-1 overflow-hidden relative">
        {!isSidebarFloating && (
          <div className="h-full flex-shrink-0">
            <Sidebar />
          </div>
        )}

        {isSidebarFloating && (
          <>
            <div
              className="absolute left-0 top-0 bottom-0 w-3 z-40"
              onMouseEnter={openFloating}
              aria-hidden="true"
            />

            <AnimatePresence>
              {showFloatingSidebar && (
                <motion.div
                  initial={reduceMotion ? false : { x: "-100%" }}
                  animate={{ x: 0 }}
                  exit={reduceMotion ? undefined : { x: "-100%" }}
                  transition={spring}
                  className="absolute left-0 top-0 bottom-0 z-[9999] h-full border-r border-zinc-800 shadow-2xl"
                  onMouseEnter={openFloating}
                  onMouseLeave={scheduleCloseFloating}
                >
                  <Sidebar />
                </motion.div>
              )}
            </AnimatePresence>
          </>
        )}
        <div className="flex flex-col flex-1 min-w-0">
          <div className="flex-1 overflow-y-auto">
            <Editor />
          </div>
          <AnimatePresence initial={false}>
            {currentNote && currentNote.note_type !== "canvas" && (
              <motion.div
                key="editor-footer"
                initial={reduceMotion ? false : { height: 0, opacity: 0 }}
                animate={{ height: "auto", opacity: 1 }}
                exit={reduceMotion ? undefined : { height: 0, opacity: 0 }}
                transition={{ duration: 0.18, ease: "easeOut" }}
                className="overflow-hidden flex-shrink-0"
              >
                <Footer wordCount={wordCount} isSaved={isSaved} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
        <AiSidebar
          isOpen={isAiSidebarOpen}
          onClose={() => setIsAiSidebarOpen(false)}
        />
      </div>
      <Toaster
        containerStyle={{
          zIndex: 99999,
        }}
        position="bottom-center"
        reverseOrder={false}
        toastOptions={{
          style: {
            background: "#333",
            color: "#fff",
            borderRadius: "10px",
          },
        }}
      />
    </div>
  );
}

export default App;
