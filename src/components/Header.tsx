import useUiStore from "../store/UiStore";
import { useNotesStore } from "../store/notesStore";
import { PanelRight, PanelLeft } from "lucide-react";

export default function Header() {
  const {
    isAiSidebarOpen,
    setIsAiSidebarOpen,
    isSidebarFloating,
    setIsSidebarFloating,
  } = useUiStore();
  const currentNote = useNotesStore((s) => s.currentNote);

  return (
    <header className="h-10 bg-zinc-950/90 px-4 border-b border-zinc-300/20 flex items-center justify-between flex-shrink-0 gap-3">
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <button
          type="button"
          onClick={() => setIsSidebarFloating(!isSidebarFloating)}
          className={`p-2 rounded-md transition-all duration-200 ${
            isSidebarFloating
              ? "text-blue-400 bg-blue-400/10"
              : "text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800"
          }`}
          title={isSidebarFloating ? "Pin sidebar" : "Float sidebar"}
          aria-label={isSidebarFloating ? "Pin sidebar" : "Float sidebar"}
          aria-pressed={isSidebarFloating}
        >
          <PanelLeft size={18} />
        </button>
        <div className="min-w-0 flex-1">
          {currentNote ? (
            <p
              className="text-sm text-zinc-300 truncate transition-opacity duration-200"
              title={currentNote.title || "Untitled"}
            >
              {currentNote.title || "Untitled"}
              {currentNote.note_type === "canvas" && (
                <span className="ml-2 text-[10px] uppercase tracking-wide text-purple-400/80">
                  Canvas
                </span>
              )}
            </p>
          ) : (
            <p className="text-sm text-zinc-600 truncate">Home</p>
          )}
        </div>
      </div>

      <div className="flex items-center flex-shrink-0">
        <button
          type="button"
          onClick={() => setIsAiSidebarOpen(!isAiSidebarOpen)}
          className={`p-2 rounded-md transition-all duration-200 ${
            isAiSidebarOpen
              ? "text-blue-400 bg-blue-400/10"
              : "text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800"
          }`}
          title={isAiSidebarOpen ? "Close AI sidebar" : "Open AI sidebar"}
          aria-label={isAiSidebarOpen ? "Close AI sidebar" : "Open AI sidebar"}
          aria-pressed={isAiSidebarOpen}
        >
          <PanelRight size={18} />
        </button>
      </div>
    </header>
  );
}
