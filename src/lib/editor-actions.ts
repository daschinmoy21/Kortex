/**
 * AI Editor Actions for BlockNote
 * Supports: Insert, Update, Delete, Replace operations
 * 
 * Industry-standard approach: separate action types for different content,
 * content sanitization, and proper node construction.
 */

const devLog = (...args: unknown[]) => { if (import.meta.env.DEV) console.log(...args); };

export interface EditorAction {
    type: "editor_action";
    action: "insertText" | "insertCode" | "insertHeading" | "insertParagraph" |
    "insertTable" | "insertList" | "update" | "delete" | "replace";
    description: string;
    data: any;
}

// =============================================================================
// CONTENT SANITIZATION UTILITIES  
// =============================================================================

/**
 * Sanitize content for HEADINGS ONLY - strips block-level elements
 * Use this ONLY for headings which cannot contain tables/lists
 */
const sanitizeForHeading = (content: any): string => {
    if (!content) return "";
    if (typeof content === "string") {
        // Only remove syntax that causes ProseMirror errors in headings
        return content
            .replace(/\|.*\|/g, '') // Remove table syntax
            .replace(/^#+\s*/gm, '') // Remove heading syntax from text
            .replace(/```[\s\S]*?```/g, '') // Remove code blocks
            .trim();
    }
    return extractPlainText(content);
};

/**
 * Extract plain text from any content structure (non-destructive)
 */
const extractPlainText = (content: any): string => {
    if (!content) return "";
    if (typeof content === "string") return content.trim();
    if (Array.isArray(content)) {
        return content
            .map((item: any) => {
                if (typeof item === "string") return item;
                if (item.text) return item.text;
                if (item.type === "text") return item.text || "";
                return "";
            })
            .join("")
            .trim();
    }
    if (content.text) return content.text;
    return "";
};

/**
 * Convert content to proper inline content array for BlockNote
 */
const toInlineContentArray = (text: string): any[] => {
    if (!text) return [];
    return [{ type: "text", text: text }];
};

/**
 * Parse table data from various formats
 */
const parseTableData = (data: any): { headers: string[], rows: string[][] } => {
    // If data includes explicit headers and rows
    if (data.headers && data.rows) {
        return { headers: data.headers, rows: data.rows };
    }

    // If data includes columns definition
    if (data.columns && Array.isArray(data.columns)) {
        const headers = data.columns.map((col: any) =>
            typeof col === "string" ? col : col.name || col.header || ""
        );
        const rows = data.rows || [];
        return { headers, rows };
    }

    // If data is markdown table format
    if (typeof data.content === "string" && data.content.includes("|")) {
        const lines = data.content.split("\n").filter((l: string) => l.trim());
        const headers = lines[0]?.split("|").map((s: string) => s.trim()).filter(Boolean) || [];
        const rows = lines.slice(2).map((line: string) =>
            line.split("|").map((s: string) => s.trim()).filter(Boolean)
        );
        return { headers, rows };
    }

    // Fallback: try to create from text
    if (data.text) {
        return { headers: [data.text], rows: [] };
    }

    return { headers: ["Column 1", "Column 2"], rows: [["", ""]] };
};

export const handleAiAction = async (editor: any, action: EditorAction) => {
    if (!editor) {
        console.error("Editor instance not found");
        throw new Error("Editor not connected");
    }

    devLog("Executing AI Action:", JSON.stringify(action, null, 2));

    try {
        switch (action.action) {
            // === INSERT ACTIONS ===
            case "insertText":
                handleInsertText(editor, action.data);
                break;
            case "insertCode":
                handleInsertCodeBlock(editor, action.data);
                break;
            case "insertHeading":
                handleInsertHeading(editor, action.data);
                break;
            case "insertParagraph":
                handleInsertParagraph(editor, action.data);
                break;
            case "insertTable":
                handleInsertTable(editor, action.data);
                break;
            case "insertList":
                handleInsertList(editor, action.data);
                break;

            // === EDIT ACTIONS ===
            case "update":
                handleUpdate(editor, action.data);
                break;
            case "delete":
                handleDelete(editor, action.data);
                break;
            case "replace":
                handleReplace(editor, action.data);
                break;

            default:
                // Fallback to simple text insertion
                const text = extractText(action.data);
                if (text) {
                    const refBlockId = getInsertionBlockId(editor);
                    if (refBlockId) {
                        editor.insertBlocks(
                            [{ type: "paragraph", content: toInlineContentArray(text) }],
                            refBlockId,
                            "after"
                        );
                    }
                }
        }
        devLog("Successfully executed action");
    } catch (error: any) {
        console.error("Failed to execute AI action:", error);
        // Provide more context in the error
        const actionDesc = action.description || action.action;
        throw new Error(`Failed to ${actionDesc}: ${error.message}`);
    }
};

// =============================================================================
// INSERT HANDLERS
// =============================================================================

/**
 * Insert text at cursor position (as paragraph block)
 */
const handleInsertText = (editor: any, data: any) => {
    const text = extractPlainText(data.text || data);

    if (text) {
        const refBlockId = getInsertionBlockId(editor);
        if (refBlockId) {
            editor.insertBlocks(
                [{ type: "paragraph", content: toInlineContentArray(text) }],
                refBlockId,
                "after"
            );
        }
    }
};

/**
 * Insert a code block
 */
const handleInsertCodeBlock = (editor: any, data: any) => {
    const code = data.code || data.text || extractText(data);
    const language = data.language || "plaintext";

    const refBlockId = getInsertionBlockId(editor);
    if (refBlockId) {
        editor.insertBlocks(
            [{
                type: "codeBlock",
                props: { language },
                content: code,
            }],
            refBlockId,
            "after"
        );
    }
};

/**
 * Insert a heading - SANITIZED to only allow inline text
 */
const handleInsertHeading = (editor: any, data: any) => {
    // CRITICAL: Headings can ONLY contain inline text, not tables or other blocks
    const rawText = data.text || extractText(data);
    const text = sanitizeForHeading(rawText);
    const level = Math.min(Math.max(data.level || 2, 1), 3); // Clamp to 1-3

    if (!text) {
        throw new Error("Heading text cannot be empty");
    }

    const refBlockId = getInsertionBlockId(editor);
    if (refBlockId) {
        editor.insertBlocks(
            [{
                type: "heading",
                props: { level },
                content: toInlineContentArray(text),
            }],
            refBlockId,
            "after"
        );
    }
};

/**
 * Insert a paragraph
 */
const handleInsertParagraph = (editor: any, data: any) => {
    const text = extractPlainText(data.text || extractText(data));

    const refBlockId = getInsertionBlockId(editor);
    if (refBlockId) {
        editor.insertBlocks(
            [{
                type: "paragraph",
                content: toInlineContentArray(text),
            }],
            refBlockId,
            "after"
        );
    }
};

/**
 * Insert a table with proper structure
 */
const handleInsertTable = (editor: any, data: any) => {
    const { headers, rows } = parseTableData(data);

    // Build table content with proper structure
    const tableContent: any = {
        type: "tableContent",
        rows: []
    };

    // Add header row
    if (headers.length > 0) {
        tableContent.rows.push({
            cells: headers.map((header: string) => [{ type: "text", text: header }])
        });
    }

    // Add data rows
    for (const row of rows) {
        tableContent.rows.push({
            cells: row.map((cell: string) => [{ type: "text", text: cell }])
        });
    }

    // Ensure at least 2 rows (header + 1 data row) for a valid table
    while (tableContent.rows.length < 2) {
        tableContent.rows.push({
            cells: headers.map(() => [{ type: "text", text: "" }])
        });
    }

    const refBlockId = getInsertionBlockId(editor);
    if (refBlockId) {
        editor.insertBlocks(
            [{
                type: "table",
                content: tableContent,
            }],
            refBlockId,
            "after"
        );
    }
};

/**
 * Insert a list (bullet or numbered)
 */
const handleInsertList = (editor: any, data: any) => {
    devLog("handleInsertList received data:", JSON.stringify(data, null, 2));

    // Get items - support multiple formats from AI
    let items: string[] = [];

    // Format 1: { items: ["item1", "item2"] }
    if (data.items && Array.isArray(data.items)) {
        items = data.items.map((item: any) =>
            typeof item === "string" ? item : extractPlainText(item)
        ).filter((item: string) => item.length > 0);
        devLog("Parsed from items array:", items);
    }
    // Format 2: { text: "- item1\n- item2" } (markdown list)
    else if (data.text && typeof data.text === "string") {
        items = data.text.split('\n').map((line: string) =>
            line.replace(/^[\s]*[-*+•][\s]+/, '').replace(/^[\s]*\d+[.)]\s*/, '').trim()
        ).filter((item: string) => item.length > 0);
        devLog("Parsed from text field:", items);
    }
    // Format 3: { content: [...] } (array of items)
    else if (data.content && Array.isArray(data.content)) {
        items = data.content.map((item: any) =>
            typeof item === "string" ? item : (item.text || extractPlainText(item))
        ).filter((item: string) => item.length > 0);
        devLog("Parsed from content array:", items);
    }

    // Final fallback: if still no items, log warning
    if (items.length === 0) {
        console.warn("No list items found in data:", data);
        // Don't create empty placeholder - just return
        throw new Error("List items are required but none were provided");
    }

    const listType = data.ordered ? "numberedListItem" : "bulletListItem";

    const blocks = items.map((item: string) => ({
        type: listType,
        content: toInlineContentArray(item),
    }));

    devLog("Creating list blocks:", JSON.stringify(blocks, null, 2));

    const refBlockId = getInsertionBlockId(editor);
    if (refBlockId) {
        editor.insertBlocks(blocks, refBlockId, "after");
    }
};

// =============================================================================
// EDIT HANDLERS
// =============================================================================

/**
 * Update an existing block
 * data: { blockId?: string, searchText?: string, newContent: string, newType?: string }
 */
const handleUpdate = (editor: any, data: any) => {
    const { blockId, searchText, newContent, newType } = data;

    let targetBlockId = blockId;

    // If no blockId provided, search for block containing the text
    if (!targetBlockId && searchText) {
        targetBlockId = findBlockByContent(editor, searchText);
    }

    if (!targetBlockId) {
        throw new Error("Could not find the block to update. Try specifying the text more precisely.");
    }

    const update: any = {};
    if (newContent) {
        // Sanitize content for the target block type
        const block = getBlockById(editor, targetBlockId);
        if (block?.type === "heading" || block?.type === "paragraph") {
            update.content = toInlineContentArray(extractPlainText(newContent));
        } else {
            update.content = newContent;
        }
    }
    if (newType) update.type = newType;

    editor.updateBlock(targetBlockId, update);
    devLog(`Updated block ${targetBlockId}`);
};

/**
 * Delete blocks
 * data: { blockIds?: string[], searchText?: string }
 */
const handleDelete = (editor: any, data: any) => {
    const { blockIds, searchText } = data;

    let idsToDelete: string[] = blockIds || [];

    // If no blockIds provided, search for block containing the text
    if (idsToDelete.length === 0 && searchText) {
        const foundId = findBlockByContent(editor, searchText);
        if (foundId) {
            idsToDelete = [foundId];
        }
    }

    if (idsToDelete.length === 0) {
        throw new Error("Could not find the block(s) to delete.");
    }

    editor.removeBlocks(idsToDelete);
    devLog(`Deleted blocks: ${idsToDelete.join(", ")}`);
};

/**
 * Replace blocks with new content
 * data: { blockIds?: string[], searchText?: string, newBlocks?: Array<{type, content}>, newContent?: string }
 */
const handleReplace = (editor: any, data: any) => {
    const { blockIds, searchText, newBlocks, newContent } = data;

    let idsToReplace: string[] = blockIds || [];

    // If no blockIds provided, search for block containing the text
    if (idsToReplace.length === 0 && searchText) {
        const foundId = findBlockByContent(editor, searchText);
        if (foundId) {
            idsToReplace = [foundId];
        }
    }

    if (idsToReplace.length === 0) {
        throw new Error("Could not find the block(s) to replace.");
    }

    // Build replacement blocks with proper content sanitization
    let replacementBlocks: any[] = [];

    if (newBlocks && Array.isArray(newBlocks)) {
        replacementBlocks = newBlocks.map(b => {
            const blockType = b.type || "paragraph";
            const content = typeof b.content === "string" ? b.content : extractText(b);

            // Sanitize based on block type
            if (blockType === "heading" || blockType === "paragraph") {
                return {
                    type: blockType,
                    content: toInlineContentArray(extractPlainText(content)),
                    ...(b.props ? { props: b.props } : {})
                };
            }
            return {
                type: blockType,
                content: content,
                ...(b.props ? { props: b.props } : {})
            };
        });
    } else if (newContent) {
        replacementBlocks = [{
            type: "paragraph",
            content: toInlineContentArray(extractPlainText(newContent))
        }];
    }

    if (replacementBlocks.length === 0) {
        throw new Error("No replacement content provided.");
    }

    editor.replaceBlocks(idsToReplace, replacementBlocks);
    devLog(`Replaced blocks: ${idsToReplace.join(", ")}`);
};

// =============================================================================
// UTILITIES
// =============================================================================

/**
 * Find a block by its content (fuzzy search)
 */
const findBlockByContent = (editor: any, searchText: string): string | null => {
    let foundId: string | null = null;
    const searchLower = searchText.toLowerCase().trim();

    editor.forEachBlock((block: any) => {
        if (foundId) return false; // Already found, stop searching

        const blockContent = getBlockTextContent(block);
        if (blockContent.toLowerCase().includes(searchLower)) {
            foundId = block.id;
            return false; // Stop traversal
        }
        return true; // Continue traversal
    });

    return foundId;
};

/**
 * Get a block by ID
 */
const getBlockById = (editor: any, blockId: string): any | null => {
    let foundBlock: any = null;

    editor.forEachBlock((block: any) => {
        if (foundBlock) return false;
        if (block.id === blockId) {
            foundBlock = block;
            return false;
        }
        return true;
    });

    return foundBlock;
};

/**
 * Get text content from a block
 */
const getBlockTextContent = (block: any): string => {
    if (!block.content) return "";
    if (typeof block.content === "string") return block.content;
    if (Array.isArray(block.content)) {
        return block.content.map((c: any) => c.text || "").join("");
    }
    return "";
};

/**
 * Get the reference block ID for insertion
 * Uses cursor position if available, falls back to last block
 * This is how industry tools like Cursor/Copilot handle insertions
 */
const getInsertionBlockId = (editor: any): string | null => {
    try {
        // Try to get current cursor position (industry standard approach)
        const cursorPos = editor.getTextCursorPosition();
        if (cursorPos?.block?.id) {
            return cursorPos.block.id;
        }
    } catch (e) {
        // Cursor API might not be available in all contexts
    }

    // Fallback to last block in document
    const doc = editor.document;
    if (doc && doc.length > 0) {
        return doc[doc.length - 1].id;
    }
    return null;
};

/**
 * Extract plain text from any data structure
 */
const extractText = (data: any): string => {
    if (!data) return "";
    if (typeof data === "string") return data;
    if (data.text) return data.text;
    if (data.code) return data.code;
    if (data.content) {
        if (typeof data.content === "string") return data.content;
        if (Array.isArray(data.content)) {
            return data.content.map((item: any) => {
                if (typeof item === "string") return item;
                if (item.text) return item.text;
                return "";
            }).join("");
        }
    }
    if (data.blocks && Array.isArray(data.blocks)) {
        return data.blocks.map((block: any) => extractText(block)).join("\n\n");
    }
    return "";
};
