// import { useState } from "react";
// import reactLogo from "./assets/react.svg";
// import { invoke } from "@tauri-apps/api/core";
import Editor from "./components/Editor.tsx";
import Header from "./components/Header.tsx";
import "./App.css";
import { Sidebar } from "./components/Sidebar.tsx";
import { useEffect, useState } from "react";
import useUiStore from "./store/UiStore.ts";
import { CommandPalette } from "./components/CommandPalette.tsx";
import Footer from "./components/Footer.tsx";
import AiSidebar from "./components/AiSidebar.tsx";
import { Settings } from "./components/Settings.tsx";

import { useNotesStore } from "./store/notesStore";
import { Toaster } from "react-hot-toast";
import PreflightModal from "./components/PrereflightModal.tsx";
import { countWordsInNoteContent } from "./lib/note-utils";

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




  useEffect(() => {
    // Load API key immediately on app startup
    loadApiKey();

    const handleKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.altKey) && event.key === "p") {
        event.preventDefault();
        openCommandPalette();
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [openCommandPalette, loadApiKey]);

  return (
    <div className="bg-zinc-950 flex flex-col h-screen overflow-hidden">
      <PreflightModal />
      <Settings />
      <CommandPalette />
      <Header />

      <div className="flex flex-1 overflow-hidden relative">
        {/* === Normal Sidebar === */}
        {!isSidebarFloating && (
          <div className="h-full flex-shrink-0">
            <Sidebar />
          </div>
        )}

        {/* === Floating Sidebar Logic === */}
        {isSidebarFloating && (
          <>
            {/* Hover Trigger Zone */}
            <div
              className="absolute left-0 top-0 bottom-0 w-10 z-40 hover:bg-transparent"
              onMouseEnter={() => setShowFloatingSidebar(true)}
            />

            <AnimatePresence>
              {showFloatingSidebar && (
                <motion.div
                  initial={{ x: "-100%" }}
                  animate={{ x: 0 }}
                  exit={{ x: "-100%" }}
                  transition={{ type: "spring", stiffness: 300, damping: 30 }}
                  className="absolute left-0 top-0 bottom-0 z-[9999] h-full border-r border-zinc-800 shadow-2xl"
                  onMouseLeave={() => setShowFloatingSidebar(false)}
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
          {currentNote && currentNote.note_type !== "canvas" && (
            <Footer wordCount={wordCount} isSaved={isSaved} />
          )}
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
