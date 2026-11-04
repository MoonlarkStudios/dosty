# The Markdown field guide

Simple marks. Beautiful words. The active heading shows its markers, and the inline span under your caret reveals its syntax. Move out of that span to conceal its marks immediately. Hidden marks leave no gaps; nearby text may reflow while the source caret, selection, and viewport remain anchored. Headings, tables, and images render directly in this live editor. Use **Source** to inspect the exact Markdown.

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
| Cmd/Ctrl + S | Save document |
| Cmd/Ctrl + F | Find and replace |
| Cmd/Ctrl + B | Bold selection |
| Cmd/Ctrl + I | Italic selection |
| Cmd/Ctrl + K | Create or edit a link |
| Cmd/Ctrl + Enter | Cell line break while editing; outside tables, toggle the task on the caret’s line |
| Cmd/Ctrl + Shift + X | Toggle strikethrough |
| Cmd/Ctrl + Shift + N | Create or edit a footnote |
| Cmd/Ctrl + Shift + T | Table row/column controls and alignment |
| Cmd/Ctrl + Shift + M | Toggle source |
| Cmd/Ctrl + Shift + F | Focus mode |
| Cmd/Ctrl + Shift + J | Search all headings |
| Cmd/Ctrl + Alt + Up / Down | Move the current section |
| Cmd/Ctrl + Alt + Left / Right | Collapse / expand its outline branch |
| Cmd/Ctrl + Z | Undo |
| Cmd/Ctrl + Shift + Z | Redo |

## Tables and images, without leaving the page

Tables normally show a bold header and subtle row rules. Click a cell to reveal its grid, row numbers, column letters, and editing controls without moving the text. Type to replace its value. Double-click or press Enter to edit the existing text. Arrows move between cells. Up from the header and Down from the last row move into text or a blank paragraph outside the table. At a document edge, the arrow creates a missing paragraph and places the caret there. Creating it is one undo step; your typing is a separate step. Shift+Arrow extends a range. Inside text editing, arrows move the text caret. Enter commits and moves down, Shift+Enter moves up, and Tab or Shift+Tab commits and moves right or left. Tab remembers the entry column for the next Enter. Cmd/Ctrl+Enter adds a cell line break.

Escape cancels pending typing without changing the document, then collapses a selected range, then hides the table controls. Click surrounding prose to commit and return to the quiet table presentation. **Leave table** starts a paragraph after it.

Drag, Shift-click, or use Shift+Arrow to select a rectangle. Click row numbers or column letters to select whole rows or columns; Shift+Space selects a row and Ctrl+Space selects a column. Delete and Backspace clear selected values while keeping the grid. Tab traverses a selected range across rows; Enter after editing traverses down columns. Each committed cell or range edit is one undo step. Markdown markers remain concealed; Source shows the exact syntax.

Use **Cmd/Ctrl+C, X, and V** to copy, cut, and paste selected cells with spreadsheet applications. Tabs, quotes, line breaks, and empty values keep their cell boundaries. Numeric-looking IDs and formula-like text stay literal when pasted into Google Sheets. Paste at one selected cell to grow the table as needed. Paste one value over a range to fill it, or repeat a smaller block across a range that is an exact multiple of its size. An incompatible range stays unchanged. The pasted rectangle remains selected, and one Undo restores the table. While editing text inside a cell, paste stays in that cell. Transfers carry values; spreadsheet formulas and formatting are not retained.

Hover a row number or column letter to reveal a small menu arrow. Click the arrow for row or column actions; clicking the label still selects it. Right-click opens the same menus. Right-click a cell, click the table icon, or press **Cmd/Ctrl+Shift+T** or **Shift+F10** to open cell actions. It keeps your current range when you click inside it. Use Up/Down or Tab/Shift+Tab to choose, Right or Enter to open a group, and Enter to apply. The selected action scrolls into view. Left, Back, or Escape returns from a group; Escape at the first menu or a click outside dismisses it. Switching to another app also dismisses the menu. Source keeps a keyboard-operated table dialog. Inserting above the header creates a new header; deleting it promotes the next row. Deleting the only row or column removes the table.

Paste or drop an image, or use **Image** to choose a file or URL. Save once to keep local images beside your Markdown. Images fit the column and remain selectable objects. Double-click an image to edit its alt text, caption, and photo credit. Save As carries relative assets to the new folder, and HTML export embeds local images in one file.

## Give a clear credit

Use **Footnote** or Cmd/Ctrl+Shift+N to add a note after the selected passage or
edit the note at the caret. Resolved references show document-wide numbers;
editing one reveals its original syntax. Cmd/Ctrl-click a reference to preview
and edit its definition. **Go to definition** and **Back to reference** move
between them without changing undo history. Definitions stay ordinary Markdown:
indent continuation paragraphs and lists by four spaces. Missing references stay
literal, and duplicate definitions can be resolved directly in Source.

Select a phrase and press Cmd/Ctrl+K to add or edit a link. Normal clicks keep you editing; Cmd/Ctrl-click opens the destination. Pasting a web URL onto selected text creates a link. Use **❝** to add a quotation with an author and source. Credits remain ordinary Markdown links and text.

Open **The quiet hour · sample** in the sidebar to try the whole workflow. **Recovery copies** holds idle checkpoints of unsaved writing; opening a copy preserves the saved-file conflict check. Save regularly, and export HTML when you want a portable reading copy.

## Current limits

Dosty is experimental. Mathematics, diagrams, and PDF/office export are
unsupported. Screen-reader support and native input-method validation remain
incomplete.
