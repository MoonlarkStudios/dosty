//! A source-coordinate accessibility tree without document mutation or native UI.

use std::ops::Range;
use std::sync::Arc;

use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Rect, Role, TextPosition, TextSelection, Tree,
    TreeUpdate,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::selection::Selection;

const ROOT: NodeId = NodeId(0);

/// Source revisions and editor modes receive a new epoch. Selection and focus
/// updates retain it, so a queued action cannot target another text snapshot.
#[derive(Clone)]
pub(super) struct Snapshot {
    source: Arc<str>,
    runs: Arc<[Run]>,
    epoch: u64,
    selection: Selection,
    enabled: bool,
    focused: bool,
    bounds: iced::Rectangle,
}

#[derive(Clone)]
struct Run {
    range: Range<usize>,
    character_lengths: Box<[u8]>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Intent {
    Focus,
    Select(Selection),
    Replace(String),
}

impl Snapshot {
    pub(super) fn new(
        source: &str,
        selection: Selection,
        epoch: u64,
        enabled: bool,
        focused: bool,
        bounds: iced::Rectangle,
    ) -> Self {
        // Each epoch reserves 2^32 IDs. The 16 MiB document limit leaves room
        // for every physical line; exhausting epochs is a process invariant.
        assert!(
            epoch <= u64::from(u32::MAX),
            "accessibility epoch exhausted"
        );
        Self {
            source: Arc::from(source),
            // AccessKit stores each selectable character's byte length in a
            // u8. A larger grapheme cannot be represented without inventing
            // cursor stops, so that document uses value-only accessibility.
            runs: source_runs(source).unwrap_or_default().into(),
            epoch,
            selection,
            enabled,
            focused,
            bounds,
        }
    }

    pub(super) fn update_state(
        &mut self,
        selection: Selection,
        enabled: bool,
        focused: bool,
        bounds: iced::Rectangle,
    ) {
        self.selection = selection;
        self.enabled = enabled;
        self.focused = focused;
        self.bounds = bounds;
    }

    pub(super) fn tree(&self, title: &str) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label(title);
        if !self.enabled {
            return TreeUpdate {
                nodes: vec![(ROOT, root)],
                tree: Some(Tree::new(ROOT)),
                focus: ROOT,
            };
        }
        root.set_children([self.editor_id()]);

        let mut editor = Node::new(Role::MultilineTextInput);
        editor.set_label("Markdown source");
        // Unlike single-line inputs, AccessKit does not synthesize a multiline
        // control's AXValue from its text-run children.
        editor.set_value(self.source.as_ref());
        editor.set_bounds(Rect::new(
            f64::from(self.bounds.x),
            f64::from(self.bounds.y),
            f64::from(self.bounds.x + self.bounds.width),
            f64::from(self.bounds.y + self.bounds.height),
        ));
        editor.set_children(
            (0..self.runs.len())
                .map(|index| self.run_id(index))
                .collect::<Vec<_>>(),
        );
        editor.add_action(Action::Focus);
        if !self.runs.is_empty() {
            editor.add_action(Action::SetTextSelection);
        }
        editor.add_action(Action::SetValue);
        if let (Some(anchor), Some(focus)) = (
            self.position(self.selection.anchor),
            self.position(self.selection.head),
        ) {
            editor.set_text_selection(TextSelection { anchor, focus });
        }

        let mut nodes = Vec::with_capacity(self.runs.len() + 2);
        nodes.push((ROOT, root));
        nodes.push((self.editor_id(), editor));
        for (index, run) in self.runs.iter().enumerate() {
            let mut node = Node::new(Role::TextRun);
            node.set_value(&self.source[run.range.clone()]);
            node.set_character_lengths(run.character_lengths.clone());
            // One run describes one physical source line. There are no other
            // runs on that line, so previous/next_on_line must remain absent.
            // Spatial text geometry is deliberately absent until it can use
            // the actual shaped source widget's glyphs and visual wraps.
            nodes.push((self.run_id(index), node));
        }
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(ROOT)),
            focus: if self.focused { self.editor_id() } else { ROOT },
        }
    }

    pub(super) fn action(&self, request: &ActionRequest) -> Option<Intent> {
        if !self.enabled || request.target != self.editor_id() {
            return None;
        }
        match (&request.action, &request.data) {
            (Action::Focus, None) => Some(Intent::Focus),
            (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                Some(Intent::Select(Selection {
                    anchor: self.source_position(selection.anchor)?,
                    head: self.source_position(selection.focus)?,
                }))
            }
            (Action::SetValue, Some(ActionData::Value(value))) => {
                Some(Intent::Replace(value.to_string()))
            }
            _ => None,
        }
    }

    fn editor_id(&self) -> NodeId {
        NodeId((self.epoch << 32) | 1)
    }

    fn run_id(&self, index: usize) -> NodeId {
        // Every run starts at a distinct byte (apart from the one empty source
        // run), and documents are capped at 16 MiB before reaching this model.
        let local = u32::try_from(index + 2).expect("document line count must fit a node ID");
        NodeId((self.epoch << 32) | u64::from(local))
    }

    fn position(&self, offset: usize) -> Option<TextPosition> {
        if offset > self.source.len() || !self.source.is_char_boundary(offset) {
            return None;
        }
        // At a physical line boundary prefer the start of the following run.
        // AccessKit expects a caret before a hard break, never after that
        // break in the preceding run.
        let index = self
            .runs
            .partition_point(|run| run.range.start <= offset)
            .checked_sub(1)?;
        let run = &self.runs[index];
        let relative = offset.checked_sub(run.range.start)?;
        let mut cursor = 0;
        for (character_index, length) in run.character_lengths.iter().enumerate() {
            if cursor == relative {
                return Some(TextPosition {
                    node: self.run_id(index),
                    character_index,
                });
            }
            cursor += usize::from(*length);
        }
        (cursor == relative).then(|| TextPosition {
            node: self.run_id(index),
            character_index: run.character_lengths.len(),
        })
    }

    fn source_position(&self, position: TextPosition) -> Option<usize> {
        if position.node.0 >> 32 != self.epoch {
            return None;
        }
        let index =
            usize::try_from((position.node.0 & u64::from(u32::MAX)).checked_sub(2)?).ok()?;
        let run = self.runs.get(index)?;
        let lengths = run.character_lengths.get(..position.character_index)?;
        Some(
            run.range.start
                + lengths
                    .iter()
                    .map(|length| usize::from(*length))
                    .sum::<usize>(),
        )
    }
}

fn source_runs(source: &str) -> Option<Vec<Run>> {
    let mut runs = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        if matches!(bytes[cursor], b'\r' | b'\n') {
            cursor +=
                if bytes[cursor..].starts_with(b"\r\n") || bytes[cursor..].starts_with(b"\n\r") {
                    2
                } else {
                    1
                };
            runs.push(source_run(source, start..cursor)?);
            start = cursor;
        } else {
            cursor += 1;
        }
    }
    // An empty source and the empty physical line after a terminal break must
    // still have a TextRun so AccessKit supports caret/text-range queries.
    runs.push(source_run(source, start..source.len())?);
    Some(runs)
}

fn source_run(source: &str, range: Range<usize>) -> Option<Run> {
    let text = &source[range.clone()];
    let (text, ending) = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix("\n\r"))
        .map_or((text, None), |text| (text, Some(2)));
    let mut character_lengths = text
        .graphemes(true)
        .map(|grapheme| u8::try_from(grapheme.len()).ok())
        .collect::<Option<Vec<_>>>()?;
    if let Some(ending) = ending {
        character_lengths.push(ending);
    }
    Some(Run {
        range,
        character_lengths: character_lengths.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(source: &str, selection: Selection, epoch: u64) -> Snapshot {
        Snapshot::new(
            source,
            selection,
            epoch,
            true,
            true,
            iced::Rectangle {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 400.0,
            },
        )
    }

    fn select(snapshot: &Snapshot, anchor: usize, head: usize) -> ActionRequest {
        ActionRequest {
            action: Action::SetTextSelection,
            target: snapshot.editor_id(),
            data: Some(ActionData::SetTextSelection(TextSelection {
                anchor: snapshot.position(anchor).unwrap(),
                focus: snapshot.position(head).unwrap(),
            })),
        }
    }

    #[test]
    fn source_text_and_directed_grapheme_selections_round_trip() {
        let source = "# 日本語 🦀\r\ne\u{301} 👩🏽‍💻\n中文\r한국어\n\rEnd";
        let snapshot = model(
            source,
            Selection {
                anchor: source.len(),
                head: 2,
            },
            7,
        );
        let tree = snapshot.tree("Test.md");
        let editor = &tree.nodes[1].1;
        assert_eq!(editor.role(), Role::MultilineTextInput);
        assert_eq!(editor.value(), Some(source));
        assert_eq!(editor.bounds(), Some(Rect::new(10.0, 20.0, 310.0, 420.0)));
        assert_eq!(tree.focus, snapshot.editor_id());
        let restored: String = tree.nodes[2..]
            .iter()
            .map(|(_, run)| run.value().unwrap())
            .collect();
        assert_eq!(restored, source);
        for (_, run) in &tree.nodes[2..] {
            assert_eq!(run.role(), Role::TextRun);
            assert_eq!(
                run.character_lengths()
                    .iter()
                    .map(|length| usize::from(*length))
                    .sum::<usize>(),
                run.value().unwrap().len()
            );
            assert!(run.character_positions().is_none());
        }
        for offset in source
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain([source.len()])
        {
            if source[..offset].ends_with('\n') && source[offset..].starts_with('\r') {
                assert!(snapshot.position(offset).is_none());
                continue;
            }
            assert_eq!(
                snapshot.action(&select(&snapshot, source.len(), offset)),
                Some(Intent::Select(Selection {
                    anchor: source.len(),
                    head: offset
                }))
            );
        }
        let selection = editor.text_selection().unwrap();
        assert_eq!(
            snapshot.source_position(selection.anchor),
            Some(source.len())
        );
        assert_eq!(snapshot.source_position(selection.focus), Some(2));
    }

    #[test]
    fn empty_and_terminal_lines_have_exact_text_positions() {
        for (source, values) in [
            ("", vec![""]),
            ("a", vec!["a"]),
            ("a\n", vec!["a\n", ""]),
            ("\r\n\r\n", vec!["\r\n", "\r\n", ""]),
            ("\n\r\n\r", vec!["\n\r", "\n\r", ""]),
            ("a\rb\nc\r\n", vec!["a\r", "b\n", "c\r\n", ""]),
        ] {
            let snapshot = model(source, Selection::caret(source.len()), 1);
            let tree = snapshot.tree("Test.md");
            assert_eq!(
                tree.nodes[2..]
                    .iter()
                    .map(|(_, run)| run.value().unwrap())
                    .collect::<Vec<_>>(),
                values
            );
            assert_eq!(
                snapshot.action(&select(&snapshot, source.len(), source.len())),
                Some(Intent::Select(Selection::caret(source.len())))
            );
            assert_eq!(
                snapshot.position(source.len()).unwrap().node,
                snapshot.run_id(snapshot.runs.len() - 1)
            );
        }
    }

    #[test]
    fn interior_grapheme_and_paired_ending_selections_are_not_moved() {
        let source = "Ae\u{301}👩🏽‍💻🇨🇦\r\nZ\n\r";
        for offset in source.char_indices().map(|(offset, _)| offset) {
            let valid = source
                .grapheme_indices(true)
                .any(|(start, _)| start == offset)
                && !(source[..offset].ends_with('\n') && source[offset..].starts_with('\r'));
            let selection = Selection::caret(offset);
            let snapshot = model(source, selection, 1);
            assert_eq!(
                snapshot.position(offset).is_some(),
                valid,
                "offset {offset}"
            );
            assert_eq!(snapshot.selection, selection);
            assert_eq!(
                snapshot.tree("Test").nodes[1].1.text_selection().is_some(),
                valid
            );
        }
    }

    #[test]
    fn oversized_grapheme_preserves_value_without_fabricated_cursor_stops() {
        let source = format!("Before\ne{}\nAfter", "\u{301}".repeat(130));
        assert!(
            source
                .graphemes(true)
                .any(|grapheme| grapheme.len() > usize::from(u8::MAX))
        );
        let snapshot = model(&source, Selection::caret(source.len()), 1);
        let tree = snapshot.tree("Test");
        assert_eq!(tree.nodes.len(), 2);
        let editor = &tree.nodes[1].1;
        assert_eq!(editor.value(), Some(source.as_str()));
        assert!(editor.children().is_empty());
        assert!(editor.text_selection().is_none());
        assert!(editor.supports_action(Action::Focus));
        assert!(editor.supports_action(Action::SetValue));
        assert!(!editor.supports_action(Action::SetTextSelection));
        assert!(snapshot.position(0).is_none());
        let invalid = TextPosition {
            node: snapshot.run_id(0),
            character_index: 0,
        };
        assert_eq!(
            snapshot.action(&ActionRequest {
                action: Action::SetTextSelection,
                target: snapshot.editor_id(),
                data: Some(ActionData::SetTextSelection(TextSelection {
                    anchor: invalid,
                    focus: invalid
                })),
            }),
            None
        );
        assert_eq!(
            snapshot.action(&ActionRequest {
                action: Action::SetValue,
                target: snapshot.editor_id(),
                data: Some(ActionData::Value("ordinary replacement".into())),
            }),
            Some(Intent::Replace("ordinary replacement".into()))
        );
    }

    #[test]
    fn actions_require_current_editor_and_valid_text_endpoints() {
        let snapshot = model("日本\n🦀", Selection::caret(0), 4);
        let request = select(&snapshot, 0, snapshot.source.len());
        assert!(snapshot.action(&request).is_some());
        let other = model("日本\n🦀", Selection::caret(0), 5);
        assert_eq!(other.action(&request), None);
        for target in [ROOT, snapshot.run_id(0), NodeId(1)] {
            assert_eq!(
                snapshot.action(&ActionRequest {
                    target,
                    ..request.clone()
                }),
                None
            );
        }
        for position in [
            TextPosition {
                node: snapshot.editor_id(),
                character_index: 0,
            },
            TextPosition {
                node: snapshot.run_id(100),
                character_index: 0,
            },
            TextPosition {
                node: snapshot.run_id(0),
                character_index: 100,
            },
            TextPosition {
                node: other.run_id(0),
                character_index: 0,
            },
        ] {
            assert_eq!(
                snapshot.action(&ActionRequest {
                    action: Action::SetTextSelection,
                    target: snapshot.editor_id(),
                    data: Some(ActionData::SetTextSelection(TextSelection {
                        anchor: position,
                        focus: snapshot.position(0).unwrap()
                    })),
                }),
                None
            );
        }
        for (action, data) in [
            (Action::Focus, Some(ActionData::Value("wrong".into()))),
            (Action::SetTextSelection, None),
            (
                Action::SetTextSelection,
                Some(ActionData::Value("wrong".into())),
            ),
            (Action::SetValue, Some(ActionData::NumericValue(3.0))),
            (Action::Click, None),
        ] {
            assert_eq!(
                snapshot.action(&ActionRequest {
                    action,
                    target: snapshot.editor_id(),
                    data
                }),
                None
            );
        }
        assert!(snapshot.position(1).is_none());
        assert!(snapshot.position(snapshot.source.len() + 1).is_none());
    }

    #[test]
    fn focus_value_and_availability_match_advertised_actions() {
        let mut snapshot = model("before", Selection::caret(0), 2);
        let focus = ActionRequest {
            action: Action::Focus,
            target: snapshot.editor_id(),
            data: None,
        };
        let replace = ActionRequest {
            action: Action::SetValue,
            target: snapshot.editor_id(),
            data: Some(ActionData::Value("日本語 🦀".into())),
        };
        assert_eq!(snapshot.action(&focus), Some(Intent::Focus));
        assert_eq!(
            snapshot.action(&replace),
            Some(Intent::Replace("日本語 🦀".into()))
        );
        let old_id = snapshot.editor_id();
        snapshot.update_state(Selection::caret(3), false, false, snapshot.bounds);
        assert_eq!(snapshot.editor_id(), old_id);
        assert_eq!(snapshot.action(&focus), None);
        assert_eq!(snapshot.action(&replace), None);
        let tree = snapshot.tree("Test.md");
        assert_eq!(tree.focus, ROOT);
        assert_eq!(tree.nodes.len(), 1);
        assert!(tree.nodes[0].1.children().is_empty());
        assert!(tree.nodes[0].1.value().is_none());
    }
}
