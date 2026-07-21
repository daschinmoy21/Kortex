const devLog = (...args: unknown[]) => { if (import.meta.env.DEV) console.log(...args); };

import { createGoogleGenerativeAI } from "@ai-sdk/google";
import { generateText } from "ai";
import toast from "react-hot-toast";

// Interface for BlockNote blocks (simplified for this utility)
interface Block {
    id?: string;
    type: string;
    props?: Record<string, any>;
    content?: string | any[];
    children?: Block[];
}

interface ProcessTranscriptionParams {
    transcriptionText: string;
    googleApiKey: string;
    editor?: any; // BlockNote editor instance
    updateCurrentNoteContent: (content: string) => void;
    updateCurrentNoteTitle?: (title: string) => void;
    saveCurrentNote: () => void;
    currentContent: any[]; // Fallback content if editor is not available
}

export const processTranscription = async ({
    transcriptionText,
    googleApiKey,
    editor,
    updateCurrentNoteContent,
    updateCurrentNoteTitle: _updateCurrentNoteTitle,
    saveCurrentNote,
    currentContent,
}: ProcessTranscriptionParams) => {


    // 1. Check if we have an API key
    // 1. Check if we have an API key
    if (!googleApiKey) {
        console.warn("Google API Key is missing provided to processTranscription");
        toast("API Key missing - Saved raw text", { icon: "⚠️" });
        return appendRawText(
            transcriptionText,
            editor,
            updateCurrentNoteContent,
            saveCurrentNote,
            currentContent,
        );
    }

    const toastId = toast.loading("🤖 Structuring with AI...");
    devLog("Starting AI structuring...");

    try {
        // 2. Initialize AI Model
        const google = createGoogleGenerativeAI({ apiKey: googleApiKey });
        const model = google("models/gemini-2.5-flash");

        // 3. Construct Prompt
        const systemPrompt = `You are an expert note-taker. Transform the raw transcription into a highly structured, educational note using BlockNote JSON blocks.

Your goal is to organize the information for effective learning, using the most appropriate block types.

Output MUST be a valid JSON array of blocks. Do not wrap in markdown.

Each block must have: "id" (unique string), "type", "props" (object), "content", "children" (array, usually []).

CRITICAL STRUCTURE RULES:
1. 'content': This is where the text goes. It can be a simple string OR an array of styled text objects.
   - Simple: "content": "Hello world"
   - Styled: "content": [{"type":"text", "text":"Bold", "styles":{"bold":true}}, {"type":"text", "text":" normal", "styles":{}}]
2. 'children': This is ONLY for nested blocks (like sub-bullets). It MUST NOT contain text objects.
   - CORRECT: "children": [{"id": "...", "type": "bulletListItem", ...}]
   - INCORRECT: "children": [{"type":"text", "text":"..."}] -> THIS WILL CRASH THE APP.

AVAILABLE BLOCK STRUCTURES:

1. PARAGRAPHS & QUOTES:
   - {"id": "unique-id-1", "type": "paragraph", "props": {"textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": "Simple text", "children": []}
   - {"id": "unique-id-2", "type": "paragraph", "props": {"textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": [{"type":"text", "text":"Bold Text", "styles":{"bold":true}}, {"type":"text", "text":" normal text", "styles":{}}], "children": []}
   - {"id": "unique-id-3", "type": "quote", "props": {}, "content": "Key takeaway or important definition", "children": []}

2. HEADINGS (Use hierarchy):
   - {"id": "unique-id-4", "type": "heading", "props": {"level": 1, "textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": "Main Title", "children": []}
   - {"id": "unique-id-5", "type": "heading", "props": {"level": 2, "textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": "Section Title", "children": []}
   - {"id": "unique-id-6", "type": "heading", "props": {"level": 3, "textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": "Subsection", "children": []}

3. LISTS (Use for steps, features, pros/cons):
   - {"id": "unique-id-8", "type": "bulletListItem", "props": {}, "content": "Point", "children": []}
   - {"id": "unique-id-9", "type": "numberedListItem", "props": {}, "content": "Step 1", "children": []}
   - {"id": "unique-id-10", "type": "checkListItem", "props": {}, "content": "Task", "children": []}
   - {"id": "unique-id-11", "type": "toggleListItem", "props": {}, "content": "Click to reveal detail", "children": []}
   
   *NESTING LISTS*: To nest a list item, put the child list item block inside the 'children' array of the parent.
   - {"id": "parent", "type": "bulletListItem", "content": "Parent", "children": [{"id": "child", "type": "bulletListItem", "content": "Child", "children": []}]}

4. CODE (For technical terms/snippets):
   - {"id": "unique-id-12", "type": "codeBlock", "props": {"language": "javascript", "textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": "console.log('code');", "children": []}

5. TABLES (Use for comparisons/data):
   - {"id": "unique-id-13", "type": "table", "props": {"textColor": "default", "backgroundColor": "default", "textAlignment": "left"}, "content": {"type": "tableContent", "rows": [{"cells": ["Col1", "Col2"]}, {"cells": ["Val1", "Val2"]}]}, "children": []}

RULES:
- Generate unique IDs for each block (e.g., using random strings or sequential numbers).
- Organize content logically with headings.
- Use TABLES for comparisons.
- Use CODE BLOCKS for code.
- Use BOLD text for key terms (using the content array format).
- **CRITICAL RULE: The first block MUST be a Heading Level 1 with a VERY SHORT, concise title (max 3-5 words) summarizing the note. This will be used as the filename.**`;

        // 4. Generate Content
        const { text: structuredJsonString } = await generateText({
            model: model,
            system: systemPrompt,
            prompt: transcriptionText,
        });

        devLog("AI Generation result:", structuredJsonString);

        // 5. Parse JSON
        const cleanJson = structuredJsonString
            .replace(/```json/g, "")
            .replace(/```/g, "")
            .trim();
        let newBlocks: Block[] = JSON.parse(cleanJson);

        if (!Array.isArray(newBlocks)) {
            throw new Error("AI output is not an array");
        }

        // Add a header block before the content
        const headerBlock = {
            id: Date.now().toString(),
            type: "heading",
            props: { level: 2 },
            content: "Audio Transcript",
            children: [],
        };
        newBlocks = [headerBlock, ...newBlocks];

        // 6. Insert Content
        if (editor) {
            // Direct Editor Insertion
            try {
                // Insert at end of document
                const targetBlock = editor.document[editor.document.length - 1];
                editor.insertBlocks(newBlocks, targetBlock, "after");

                // Force sync to store
                updateCurrentNoteContent(JSON.stringify(editor.document));
                saveCurrentNote();
            } catch (editorError) {
                console.warn(
                    "Editor insertion failed, falling back to store update",
                    editorError,
                );
                // Fallback to store update if editor method fails
                const updatedContent = [...currentContent, ...newBlocks];
                updateCurrentNoteContent(JSON.stringify(updatedContent));
                saveCurrentNote();
            }
        } else {
            // Editor not active, append to store content
            const updatedContent = [...currentContent, ...newBlocks];
            updateCurrentNoteContent(JSON.stringify(updatedContent));
            saveCurrentNote();
        }

        toast.success("Processed & Structured!", { id: toastId });
    } catch (error: any) {
        console.error("AI Structuring failed completely:", error);
        let errorMessage = "AI structuring failed, using raw text.";

        if (error.message?.includes("API key")) {
            errorMessage = "Invalid API Key. Using raw text.";
        } else if (error.message?.includes("fetch")) {
            errorMessage = "Network error. Using raw text.";
        }

        toast.error(errorMessage, { id: toastId });
        // Fallback to raw text
        return appendRawText(
            transcriptionText,
            editor,
            updateCurrentNoteContent,
            saveCurrentNote,
            currentContent,
        );
    }
};

const appendRawText = (
    text: string,
    editor: any,
    updateCurrentNoteContent: (content: string) => void,
    saveCurrentNote: () => void,
    currentContent: any[],
) => {
    const rawBlock = {
        id: Date.now().toString(),
        type: "paragraph",
        props: {},
        content: text,
        children: [],
    };

    if (editor) {
        try {
            const targetBlock = editor.document[editor.document.length - 1];
            editor.insertBlocks([rawBlock], targetBlock, "after");
            // Force sync
            updateCurrentNoteContent(JSON.stringify(editor.document));
            saveCurrentNote();
        } catch (e) {
            const updatedContent = [...currentContent, rawBlock];
            updateCurrentNoteContent(JSON.stringify(updatedContent));
            saveCurrentNote();
        }
    } else {
        const updatedContent = [...currentContent, rawBlock];
        updateCurrentNoteContent(JSON.stringify(updatedContent));
        saveCurrentNote();
    }
};
