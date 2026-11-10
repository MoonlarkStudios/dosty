# The Markdown field guide

Simple marks. Beautiful words. The active heading shows its markers, and the inline span under your caret reveals its syntax. Move out of that span to conceal its marks immediately. Hidden marks leave no gaps; nearby text may reflow while the source caret, selection, and viewport remain anchored. Headings, tables, and images render directly in this live editor. Choose **Source** from the document-name menu to inspect the exact Markdown.

## Your writing space

Dosty opens with one blank document and both sidebars hidden. Open a file or choose **File → Open Folder…** to use a folder as your workspace. Files and their titlebar toggle appear only while a folder workspace is open. The folder name at the top of Files opens workspace actions: switch folders, create a document or folder, refresh, show its location, or close it. On macOS, **Show in Finder** reveals the workspace folder. Closing the workspace leaves your document open. Dosty remembers each workspace: the next launch reopens the last folder with its document, caret, expanded folders and sidebars, and **File → Open Recent** returns to earlier folders.

Within a workspace, use the left titlebar button or Cmd/Ctrl+Shift+B to toggle Files. Use the right button or Cmd/Ctrl+Alt+O to toggle Outline independently. Each toggle keeps its sidebar open until you close it. When both panels cannot fit comfortably, Outline overlays the document rather than squeezing the writing column. **Hide Sidebars** (Cmd/Ctrl+Shift+F) hides both panels while retaining their open state; the same command restores them. Escape dismisses the current editing context rather than restoring hidden sidebars.

**Quick Open** (Cmd/Ctrl+P, or the search icon in Files) finds Markdown files by filename or relative path, including files in collapsed folders. Type one or more terms, use Up/Down to choose a result, and Enter to open it. Escape returns focus to where you were working. Opening a different file uses the usual Save/Discard/Cancel guard. The search is cleared when closed.

Outline offers Search, Collapse All, and Expand All icons when the document has headings. Search opens an inline filter; Cmd/Ctrl+Shift+J focuses it. Matching headings keep their parent context. Up/Down chooses a match, Enter jumps, and Escape closes the filter.

Hover a file or folder to reveal its actions, or right-click it. A folder's menu offers **New document…** and **New folder…**; items below the root also offer **Rename** and **Move to…**. **Show location** displays the full path and can open its containing folder. Drag a file or folder onto another folder, or into the tree’s empty space to move it to the workspace root. A small preview and highlighted destination show where it will go; pausing over a collapsed folder opens it. Escape cancels the drag. Moves preserve unsaved writing and text undo; collisions, cross-filesystem moves, and references that cannot be preserved produce an error. There is no delete action. **Undo move** appears in a contextual notice and remains available in the document-name menu.

In the file tree, Up/Down selects a row, Left/Right collapses or expands folders, and Enter or Space opens a file or toggles a folder. Home/End reaches the first/last row. F2 renames the selected item; Shift+F10 or the context-menu key opens its actions. Use Up/Down or Tab/Shift+Tab and Enter in menus. Hovering an item also shows its full path.

Titlebar navigation, the filename, and the save indicator stay visible while you type: a check means saved, a filled dot means unsaved changes, an empty circle means never saved, and a warning means saving failed. Click the indicator to save; hover it for details. The document-name menu holds Save, Rename, Move, and the **Write / Source / Read** modes. Source and Read show a small label beside the filename while active.

**Format** in the titlebar opens formatting and insertion commands on demand. On macOS, the native menu bar also offers File, Edit, Format, View, Window, and Help; Edit commands act on the focused editor, table draft, or text field. **Dosty → Settings…** configures appearance, writing typeface, word count, and reading time. System sans-serif is the default; choose **Writing → Typeface → Serif** for Newsreader. The typeface remains selected when switching themes. Statistics stay at the bottom center of the writing column while you type. Reading time assumes about 200 words per minute. View also exposes the writing preferences. On Windows/Linux, the in-window File menu contains the equivalent command groups. F10 opens that keyboard-operated command menu on every platform.

## The essentials

Use **bold** for emphasis and *italics* for a softer voice. Put `inline code` between backticks. Write ~~strikethrough~~ with tildes, or use the Strikethrough control or Cmd/Ctrl+Shift+X on selected text. Its markers reveal while editing, and its line appears in Live and Read views.

## Structure your thoughts

Start a heading with `#`, `##`, or `###`. Click into a heading to edit its markers while keeping the title's size. Home moves to the start of its visible markers. Separate paragraphs with a blank line. Use ordinary arrows and Shift+arrows to move and select across the whole document. Enter continues a list; Enter in an empty item exits it. Click a task checkbox to toggle it without moving the text selection. Outside tables, Cmd/Ctrl+Enter toggles the checkbox on the caret’s source line; continuation lines without a checkbox are unchanged. Alt+Up and Alt+Down jump between blocks. Escape closes find or a dialog.

> A blockquote makes a little space for another voice.

1. An ordered list
2. Another thought
   - With a nested idea

- [ ] An unfinished task
- [x] A little progress

## Show your work

```rust
fn main() {
    println!("Make room for what matters.");
}
```

| Shortcut | Action |
| :--- | :--- |
| F10 | Open the File menu |
| Cmd/Ctrl + O | Open a file |
| Cmd/Ctrl + P | Quick Open within the workspace |
| Cmd/Ctrl + Shift + O | Open a folder |
| Cmd/Ctrl + S | Save document |
| Cmd/Ctrl + Shift + B | Toggle Files |
| Cmd/Ctrl + Alt + O | Toggle Outline |
| Cmd/Ctrl + F | Find and replace |
| Cmd/Ctrl + B | Bold selection |
| Cmd/Ctrl + I | Italic selection |
| Cmd/Ctrl + K | Create or edit a link |
| Cmd/Ctrl + Enter | Cell line break while editing; outside tables, toggle the task on the caret’s line |
| Cmd/Ctrl + Shift + X | Toggle strikethrough |
| Cmd/Ctrl + Shift + N | Create or edit a footnote |
| Cmd/Ctrl + Shift + T | Table row/column controls and alignment |
| Cmd/Ctrl + Shift + M | Toggle source |
| Cmd/Ctrl + Shift + F | Hide / show sidebars |
| Cmd/Ctrl + Shift + J | Filter Outline |
| Cmd/Ctrl + Alt + Up / Down | Move the current section |
| Cmd/Ctrl + Alt + Left / Right | Collapse / expand its outline branch |
| Cmd/Ctrl + Z | Undo |
| Cmd/Ctrl + Shift + Z | Redo |

## Tables and images, without leaving the page

Tables normally show a bold header and subtle row rules. Click a cell to reveal its grid, row numbers, column letters, and editing controls without moving the text. Type to replace its value. Double-click or press Enter to edit the existing text. Arrows move between cells. Up from the header and Down from the last row move into text or a blank paragraph outside the table. At a document edge, the arrow creates a missing paragraph and places the caret there. Creating it is one undo step; your typing is a separate step. Shift+Arrow extends a range. Inside text editing, arrows move the text caret. Enter commits and moves down, Shift+Enter moves up, and Tab or Shift+Tab commits and moves right or left. Tab remembers the entry column for the next Enter. Cmd/Ctrl+Enter adds a cell line break.

Escape cancels pending typing without changing the document, then collapses a selected range, then hides the table controls. Click surrounding prose to commit and return to the quiet table presentation. **Leave table** starts a paragraph after it.

Drag, Shift-click, or use Shift+Arrow to select a rectangle. Click row numbers or column letters to select whole rows or columns; Shift+Space selects a row and Ctrl+Space selects a column. Delete and Backspace clear selected values while keeping the grid. Tab traverses a selected range across rows; Enter after editing traverses down columns. Each committed cell or range edit is one undo step. Markdown markers remain concealed; Source shows the exact syntax.

Use **Cmd/Ctrl+C, X, and V** to copy, cut, and paste selected cells with spreadsheet applications. Tabs, quotes, line breaks, and empty values keep their cell boundaries. Numeric-looking IDs and formula-like text stay literal when pasted into Google Sheets. Paste at one selected cell to grow the table as needed. Paste one value over a range to fill it, or repeat a smaller block across a range that is an exact multiple of its size. An incompatible range stays unchanged. The pasted rectangle remains selected, and one Undo restores the table. While editing text inside a cell, paste stays in that cell. Transfers carry values; spreadsheet formulas and formatting are not retained.

Hover a row number or column letter to reveal a small menu arrow. Click the arrow for row or column actions; clicking the label still selects it. Right-click opens the same menus. Right-click a cell, choose **Format → Table**, or press **Cmd/Ctrl+Shift+T** or **Shift+F10** to open cell actions. It keeps your current range when you click inside it. Use Up/Down or Tab/Shift+Tab to choose, Right or Enter to open a group, and Enter to apply. The selected action scrolls into view. Left, Back, or Escape returns from a group; Escape at the first menu or a click outside dismisses it. Switching to another app also dismisses the menu. Source keeps a keyboard-operated table dialog. Inserting above the header creates a new header; deleting it promotes the next row. Deleting the only row or column removes the table.

Paste or drop an image, or choose **Format → Image…** to choose a file or URL. Save once to keep local images beside your Markdown. Images fit the column and remain selectable objects. Double-click an image to edit its alt text, caption, and photo credit. Save As carries relative assets to the new folder, and HTML export embeds local images in one file.

## Give a clear credit

Use **Footnote** or Cmd/Ctrl+Shift+N to add a note after the selected passage or
edit the note at the caret. Resolved references show document-wide numbers;
editing one reveals its original syntax. Cmd/Ctrl-click a reference to preview
and edit its definition. **Go to definition** and **Back to reference** move
between them without changing undo history. Definitions stay ordinary Markdown:
indent continuation paragraphs and lists by four spaces. Missing references stay
literal, and duplicate definitions can be resolved directly in Source.

Select a phrase and press Cmd/Ctrl+K to add or edit a link. Normal clicks keep you editing; Cmd/Ctrl-click opens the destination. Pasting a web URL onto selected text creates a link. Choose **Format → Quotation…** to add a quotation with an author and source. Credits remain ordinary Markdown links and text.

Open **Help → The Quiet Hour** to try the whole workflow. Help also contains **Recovery Copies…** and **Markdown Guide**; appearance lives in Settings. Windows/Linux expose these under **File → Help & settings…**. Recovery copies hold idle checkpoints of unsaved writing; opening a copy preserves the saved-file conflict check. Save regularly, and choose **File → Export HTML…** when you want a portable reading copy.

## Current limits

Dosty is experimental. Mathematics, diagrams, and PDF/office export are
unsupported. Screen-reader support and native input-method validation remain
incomplete.
