import { BlockNoteEditor, filterSuggestionItems } from "@blocknote/core";
import { useCreateBlockNote } from "@blocknote/react";
import { ReactNode, createContext, useContext, useRef, useMemo, useEffect } from "react";
import { codeBlock } from "@blocknote/code-block";
import {
  createAIExtension,
  getAISlashMenuItems,
} from "@blocknote/xl-ai";
import { en as aiEn } from "@blocknote/xl-ai/locales";
import "@blocknote/xl-ai/style.css";
import { en } from "@blocknote/core/locales";
import {
  FormattingToolbar,
  FormattingToolbarController,
  SuggestionMenuController,
  getDefaultReactSlashMenuItems,
  BlockTypeSelect,
  BasicTextStyleButton,
  TextAlignButton,
  ColorStyleButton,
  NestBlockButton,
  UnnestBlockButton,
  CreateLinkButton,
} from "@blocknote/react";
import { Note } from "../types/Note";
import { createGoogleGenerativeAI } from '@ai-sdk/google';
import useUiStore from '../store/UiStore';


type EditorContextType = {
  editor: any | null;
  hasAI: boolean;
  updateCurrentNoteTitle: (title: string) => void;
};

const EditorContext = createContext<EditorContextType | null>(null);

export function useEditorContext() {
  const context = useContext(EditorContext);
  if (!context) {
    throw new Error("useEditorContext must be used within EditorProvider");
  }
  return context;
}

// function to convert file to base64
const convertFileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    console.log("Starting base64 conversion for:", file.name, file.type, file.size);
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => {
      const result = reader.result as string;
      console.log("Base64 conversion successful. Length:", result.length, "Prefix:", result.substring(0, 50));
      resolve(result);
    };
    reader.onerror = (error) => {
      console.error("Base64 conversion failed:", error);
      reject(error);
    };
  });
};

export function EditorProvider({
  children,
  currentNote,
  updateCurrentNoteContent,
  updateCurrentNoteTitle,
}: {
  children: ReactNode;
  currentNote: Note;
  updateCurrentNoteContent: (content: string) => void;
  updateCurrentNoteTitle: (title: string) => void;
}) {
  const { googleApiKey, setEditor } = useUiStore();

  // Create model only when API key changes
  const model = useMemo(() => {
    if (googleApiKey) {
      const googleAI = createGoogleGenerativeAI({
        apiKey: googleApiKey,
      });
      return googleAI('gemini-2.5-flash');
    }
    return null;
  }, [googleApiKey]);

  const hasAI = Boolean(model);

  // Create editor only when model is available
  const editor = useCreateBlockNote({
    tables: {
      splitCells: true,
      cellBackgroundColor: true,
      cellTextColor: true,
      headers: true
    },
    codeBlock,
    dictionary: {
      ...en,
      ai: aiEn,
    },
    extensions: model ? [
      createAIExtension({
        model: model as any,
      }),
    ] : [],
    initialContent: currentNote?.content ? JSON.parse(currentNote.content) : undefined,
    uploadFile: async (file: File) => {
      console.log("uploadFile triggered for:", file);
      try {
        const base64 = await convertFileToBase64(file);
        return base64;
      } catch (e) {
        console.error("uploadFile failed:", e);
        throw e;
      }
    },
  });

  const previousNoteIdRef = useRef<string | null>(null);
  const saveTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Set editor in store
  useEffect(() => {
    setEditor(editor);
  }, [editor, setEditor]);

  // Sync: Only load content when the NOTE ID changes (switching notes)
  // We do NOT update the editor if `currentNote.content` changes while ID is the same,
  // because the Editor itself is the source of truth for the content while active.
  useEffect(() => {
    if (currentNote && currentNote.id !== previousNoteIdRef.current) {
      console.log('Switching to note:', currentNote.id);
      previousNoteIdRef.current = currentNote.id;

      try {
        const content = currentNote.content ? JSON.parse(currentNote.content) : [];
        editor.replaceBlocks(editor.document, content);
      } catch (error) {
        console.warn("Invalid JSON content, treating as plain text");
        editor.replaceBlocks(editor.document, [
          {
            type: "paragraph",
            content: currentNote.content || "",
          },
        ]);
      }
    }
  }, [currentNote?.id, editor]);
  // Removed `currentNote.content` from dependency to prevent re-entrancy loops/overwrites

  // Handle content changes with DEBOUNCE (not throttle)
  // This ensures the LAST keystroke is always saved.
  useEffect(() => {
    const handleChange = () => {
      // Clear previous timeout to reset the timer (debounce)
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }

      // Set new timeout
      saveTimeoutRef.current = setTimeout(() => {
        if (currentNote) {
          const content = JSON.stringify(editor.document);
          updateCurrentNoteContent(content);

          // Update title based on first block
          const firstBlock = editor.document[0];
          let title = "Untitled";
          if (firstBlock) {
            const content = firstBlock.content;
            if (typeof content === 'string') {
              title = content || "Untitled";
            } else if (Array.isArray(content)) {
              title = content.map((c: any) => c.text).join('') || "Untitled";
            }
          }
          // Only update if changed to avoid loop (store check might be enough but let's be safe)
          if (currentNote.title !== title) {
            updateCurrentNoteTitle(title);
          }
        }
      }, 500); // 500ms debounce
    };

    // Listen for editor changes
    editor.onChange(handleChange);

    return () => {
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }
    };
  }, [editor, currentNote, updateCurrentNoteContent]);

  return (
    <EditorContext.Provider value={{ editor, updateCurrentNoteTitle, hasAI }}>
      {children}
    </EditorContext.Provider>
  );
}

// Formatting toolbar with all standard buttons explicitly defined
export function FormattingToolbarWithAI() {
  return (
    <FormattingToolbarController
      formattingToolbar={() => (
        <FormattingToolbar>
          <BlockTypeSelect key={"blockTypeSelect"} />

          <BasicTextStyleButton basicTextStyle={"bold"} key={"boldStyleButton"} />
          <BasicTextStyleButton basicTextStyle={"italic"} key={"italicStyleButton"} />
          <BasicTextStyleButton basicTextStyle={"underline"} key={"underlineStyleButton"} />
          <BasicTextStyleButton basicTextStyle={"strike"} key={"strikeStyleButton"} />
          <BasicTextStyleButton basicTextStyle={"code"} key={"codeStyleButton"} />

          <TextAlignButton textAlignment={"left"} key={"textAlignLeftButton"} />
          <TextAlignButton textAlignment={"center"} key={"textAlignCenterButton"} />
          <TextAlignButton textAlignment={"right"} key={"textAlignRightButton"} />

          <ColorStyleButton key={"colorStyleButton"} />

          <NestBlockButton key={"nestBlockButton"} />
          <UnnestBlockButton key={"unnestBlockButton"} />

          <CreateLinkButton key={"createLinkButton"} />
        </FormattingToolbar>
      )}
    />
  );
}

// Slash menu with the AI option added
export function SuggestionMenuWithAI({ editor }: { editor: BlockNoteEditor<any, any, any> }) {
  const { googleApiKey } = useUiStore();

  return (
    <SuggestionMenuController
      triggerCharacter="/"
      getItems={async (query) =>
        filterSuggestionItems(
          [
            ...getDefaultReactSlashMenuItems(editor),
            // add the default AI slash menu items only if API key is present
            ...(googleApiKey ? getAISlashMenuItems(editor) : []),
          ],
          query,
        )
      }
    />
  );
}
