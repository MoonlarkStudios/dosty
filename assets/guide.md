# The Markdown field guide

Simple marks. Beautiful words. The active heading shows its markers, and the inline span under your caret reveals its syntax. Move out of that span to conceal its marks immediately. Hidden marks leave no gaps; nearby text may reflow while the source caret, selection, and viewport remain anchored. Headings, tables, and images render directly in this live editor. Choose **Source** from the editing mode dropdown to inspect the exact Markdown.

## Your writing space

Dosty resumes your last project and document. When there is no document to resume, Welcome shows your recent projects. Choose **New Document** to start a standalone draft or choose **Open Markdown File…** to edit an existing file. **New Project…** creates a folder at the location you choose and opens a blank draft. **Open Project…** opens a folder and asks before adding its `.dosty/project.toml` metadata. Use **File → Projects…** to return to any remembered project. Older Places appear there with their working state. **Set Up Project…** adds metadata when you choose.

To revisit Welcome with a document open, use the house icon in the titlebar. **Return to Document** or Escape takes you back to your writing.

Files shows the project’s folders and documents. The project name opens its actions, including creation, refresh, location, and removal from the list. **Show in Finder** reveals the folder on macOS. A dot marks unsaved changes beside a document. A fainter dot marks a collapsed folder with unsaved changes. Drafts appear above the tree.

Use **Files** in the titlebar or Cmd/Ctrl+Shift+B to show the project browser. Use **Outline** or Cmd/Ctrl+Alt+O to show the headings. At narrow widths, opening a sidebar brings it forward and tucks the other away. Both can return when the window widens. The writing stays fully visible. **Hide Sidebars** (Cmd/Ctrl+Shift+F) hides both panels while retaining their open state; the same command restores them. Escape dismisses the current editing context rather than restoring hidden sidebars.

**Quick Open** (Cmd/Ctrl+P, or the search icon in the titlebar) helps you move between documents. Before you type, it lists documents with unsaved changes, the documents you used most recently, the folders and documents of the open project, and your other projects. Use Up/Down to choose, Right or Enter to step into a folder, Left to step back out, and Enter to open a document. Type one or more terms to search by name or path across every project in your library. Escape returns focus to where you were working. The search is cleared when closed. Type `#` and a space to jump to a heading in the current document. Use **Pin in Quick Open** from a file's context menu to keep it above recent files for its project. The Files header search opens **Find in Folder** for document contents in the active project.

**Find in Folder** (Cmd/Ctrl+Alt+F) searches Markdown in the open project. Type a phrase and press Enter. Choose a result to select the match in its file. Escape returns to writing.

**New document** (Cmd/Ctrl+N) gives you an empty page immediately. It takes its title from its first line and belongs to the folder of the document you were in; the first save puts it there without asking, and never over an existing file. Use **Name…** in the document menu to choose the file name yourself first.

Moving to another document never interrupts you. The one you leave stays open in the background with its unsaved changes and undo history, and the titlebar arrows **Back** (Cmd/Ctrl+[) and **Forward** (Cmd/Ctrl+]) move through the documents you have visited. Dosty asks about unsaved changes only when you close the window, once, naming every unsaved document.

Outline offers Search, Collapse All, and Expand All icons when the document has headings. Search opens an inline filter; Cmd/Ctrl+Shift+J focuses it. Matching headings keep their parent context. Up/Down chooses a match, Enter jumps, and Escape closes the filter. In Write, hover next to a heading in the left margin and click its chevron to fold the section body. Cmd/Ctrl+Alt+L toggles the fold for the current section.

Files also shows the images in a project. Choose one to preview it, and use **Insert into document** there, or in its menu, to reference it from the page where it already lives; nothing is copied. Images cannot be renamed or moved here, because pages that use them would lose them.

Right-click a file or folder for its actions. Cmd-click on Mac or Ctrl-click on Windows and Linux adds or removes items, and Shift-click selects a range. Drag any selected row into a folder to move the group. Pause over a folder to expand it, or drop on **Project root** at the bottom of Files to move items to the top level. **Undo move** reverses the group separately from text undo. The file menus also offer Rename, Move to, and Show location.

In the file tree, Up/Down selects a row, Left/Right collapses or expands folders, and Enter or Space opens a file or toggles a folder. Home/End reaches the first/last row. F2 renames the selected item; Shift+F10 or the context-menu key opens its actions. Use Up/Down or Tab/Shift+Tab and Enter in menus. Hovering an item also shows its full path.

The filename menu holds file actions such as Save, Rename, and Move. Choose **Write**, **Source**, or **Read** from the mode dropdown on the right. The titlebar shows when a draft is new, a document is unsaved, or a save failed. A check means all changes are saved. Click the indicator to save or retry, and hover for details.

**Format** in the titlebar opens formatting and insertion commands on demand. On macOS, the native menu bar also offers File, Edit, Format, View, Window, and Help; Edit commands act on the focused editor, table draft, or text field. Open **Settings** from the titlebar or press Cmd/Ctrl+comma. **Writing** previews Source Sans 3, Newsreader, and System Sans and lets you set the text size, line width, and line spacing. Source Sans 3 is the default. Your writing preferences are saved independently of your color theme. **Appearance** previews the built-in themes and loads custom TOML themes. **Shortcuts** lists keyboard commands. Statistics stay at the bottom center of the writing column while you type. Select text to count its words. Click the statistics or press Cmd/Ctrl+Alt+I for document totals, including the Markdown character count. Reading time assumes about 200 words per minute. View also exposes the writing preferences. On Windows/Linux, the in-window File menu contains the equivalent command groups. F10 opens that keyboard-operated command menu on every platform.

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
| Cmd/Ctrl + P | Quick Open: move between documents and projects |
| Cmd/Ctrl + [ and ] | Back and Forward through visited documents |
| Cmd/Ctrl + Shift + O | Open a project |
| Cmd/Ctrl + S | Save document |
| Cmd/Ctrl + Shift + B | Toggle Files |
| Cmd/Ctrl + Alt + O | Toggle Outline |
| Cmd/Ctrl + F | Find and replace |
| Cmd/Ctrl + Alt + F | Find in Folder |
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

## Link files in a project

Use **Find file or heading…** in the link dialog to pick a file in the open project or
one of its headings. Apply writes a relative Markdown link. Open **Linked
from…** in the filename menu to see which files in the open project link to the current
document. Choose a result to follow it back to its source.

## Tables and images, without leaving the page

Tables normally show a bold header and subtle row rules. Click a cell to reveal its grid, row numbers, column letters, and editing controls without moving the text. Type to replace its value. Double-click or press Enter to edit the existing text. Arrows move between cells. Up from the header and Down from the last row move into text or a blank paragraph outside the table. At a document edge, the arrow creates a missing paragraph and projects the caret there. Creating it is one undo step; your typing is a separate step. Shift+Arrow extends a range. Inside text editing, arrows move the text caret. Enter commits and moves down, Shift+Enter moves up, and Tab or Shift+Tab commits and moves right or left. Tab remembers the entry column for the next Enter. Cmd/Ctrl+Enter adds a cell line break.

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
