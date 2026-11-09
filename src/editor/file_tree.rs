//! Folder navigation owns transient pointer/focus geometry, never document state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::text::{self, Paragraph as _, Renderer as _};
use iced::advanced::widget::{self, Tree, operation};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard::{self, Key, key};
use iced::widget::canvas::{Frame, LineCap, Path as CanvasPath, Stroke};
use iced::{
    Border, Color, Element, Event, Font, Length, Point, Rectangle, Renderer, Shadow, Size, Theme,
    Vector, window,
};

use crate::theme::Tokens;
use crate::theme::styles;

pub(crate) const ID: &str = "folder-file-tree";
const ROW_HEIGHT: f32 = styles::SIDEBAR_ROW_HEIGHT;
const INDENT: f32 = styles::SIDEBAR_INDENT;
const DRAG_THRESHOLD: f32 = 7.0;
const HOVER_DELAY: Duration = Duration::from_millis(600);

/// A visible row; callers omit the descendants of collapsed folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub path: PathBuf,
    pub label: String,
    pub depth: usize,
    pub directory: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Focus,
    Open(PathBuf),
    Toggle(PathBuf),
    Expand(PathBuf),
    Rename(PathBuf),
    Menu {
        path: PathBuf,
        position: Point,
    },
    Move {
        source: PathBuf,
        destination: PathBuf,
    },
}

pub(crate) struct FileTree<'a, Message> {
    root: Option<&'a Path>,
    entries: &'a [Entry],
    active: Option<&'a Path>,
    highlighted: Option<&'a Path>,
    tokens: Tokens,
    on_action: Box<dyn Fn(Action) -> Message + 'a>,
    enabled: bool,
}

impl<'a, Message> FileTree<'a, Message> {
    pub fn new(
        entries: &'a [Entry],
        active: Option<&'a Path>,
        tokens: Tokens,
        on_action: impl Fn(Action) -> Message + 'a,
    ) -> Self {
        Self {
            root: None,
            entries,
            active,
            highlighted: None,
            tokens,
            on_action: Box::new(on_action),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn root(mut self, root: &'a Path) -> Self {
        self.root = Some(root);
        self
    }

    fn valid_destination(&self, source: &Path, destination: &Path) -> bool {
        valid_destination(source, destination, self.entries, self.root)
    }

    pub fn highlighted(mut self, path: Option<&'a Path>) -> Self {
        self.highlighted = path;
        self
    }

    fn publish(&self, shell: &mut Shell<'_, Message>, action: Action) {
        shell.publish((self.on_action)(action));
    }

    fn reconcile(&self, state: &mut State) {
        let root = self
            .root
            .map(Path::to_owned)
            .or_else(|| self.entries.first().map(|entry| entry.path.clone()));
        if state.root != root {
            *state = State {
                root,
                ..State::default()
            };
        }
        if state
            .focus_path
            .as_ref()
            .is_none_or(|path| !self.entries.iter().any(|entry| entry.path == *path))
        {
            state.focus_path = self
                .highlighted
                .and_then(|path| self.entries.iter().find(|entry| entry.path == path))
                .or_else(|| {
                    self.active
                        .and_then(|path| self.entries.iter().find(|entry| entry.path == path))
                })
                .or_else(|| self.entries.first())
                .map(|entry| entry.path.clone());
        }
        if !self.enabled {
            state.cancel_drag();
        }
        if state
            .hovered
            .as_ref()
            .is_some_and(|path| !self.entries.iter().any(|entry| entry.path == *path))
        {
            state.hovered = None;
            state.hover_since = None;
        }
    }

    fn hovered(
        &self,
        state: &State,
        bounds: Rectangle,
        viewport: Rectangle,
        point: Point,
    ) -> Option<usize> {
        if !bounds.contains(point) || !viewport.contains(point) {
            return None;
        }
        row_at(point.y - bounds.y, state.scroll, self.entries.len())
    }

    fn hover_target(
        &self,
        state: &mut State,
        bounds: Rectangle,
        viewport: Rectangle,
        now: Instant,
    ) {
        let target = state
            .pointer
            .and_then(|point| self.hovered(state, bounds, viewport, point))
            .filter(|index| self.entries[*index].directory)
            .map(|index| self.entries[index].path.clone())
            .or_else(|| {
                state
                    .pointer
                    .filter(|point| bounds.contains(*point) && viewport.contains(*point))
                    .filter(|point| self.hovered(state, bounds, viewport, *point).is_none())
                    .and(self.root)
                    .map(Path::to_owned)
            });
        if state.target != target {
            state.target = target;
            state.target_since = Some(now);
            state.expansion_sent = false;
        }
    }

    fn focused_index(&self, state: &State) -> Option<usize> {
        state
            .focus_path
            .as_ref()
            .and_then(|path| self.entries.iter().position(|entry| entry.path == *path))
    }

    fn keyboard(
        &self,
        state: &mut State,
        key: &Key,
        modifiers: keyboard::Modifiers,
        bounds: Rectangle,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        let Some(index) = self.focused_index(state) else {
            return false;
        };
        let entry = &self.entries[index];
        let next = match key {
            Key::Named(key::Named::ArrowDown) => {
                Some((index + 1).min(self.entries.len().saturating_sub(1)))
            }
            Key::Named(key::Named::ArrowUp) => Some(index.saturating_sub(1)),
            Key::Named(key::Named::Home) => Some(0),
            Key::Named(key::Named::End) => Some(self.entries.len().saturating_sub(1)),
            Key::Named(key::Named::ArrowRight) if entry.directory => {
                if !entry.expanded {
                    self.publish(shell, Action::Expand(entry.path.clone()));
                    None
                } else if self
                    .entries
                    .get(index + 1)
                    .is_some_and(|child| child.depth > entry.depth)
                {
                    Some(index + 1)
                } else {
                    None
                }
            }
            Key::Named(key::Named::ArrowRight) => None,
            Key::Named(key::Named::ArrowLeft) => {
                if entry.directory && entry.expanded {
                    self.publish(shell, Action::Toggle(entry.path.clone()));
                    None
                } else {
                    self.entries[..index]
                        .iter()
                        .rposition(|parent| parent.depth < entry.depth)
                }
            }
            Key::Named(key::Named::Enter | key::Named::Space) => {
                self.publish(
                    shell,
                    if entry.directory {
                        Action::Toggle(entry.path.clone())
                    } else {
                        Action::Open(entry.path.clone())
                    },
                );
                None
            }
            Key::Named(key::Named::F2) if state.root.as_deref() != Some(entry.path.as_path()) => {
                self.publish(shell, Action::Rename(entry.path.clone()));
                None
            }
            Key::Named(key::Named::ContextMenu) => {
                self.publish(
                    shell,
                    Action::Menu {
                        path: entry.path.clone(),
                        position: row_bounds(bounds, index, state.scroll).center(),
                    },
                );
                None
            }
            Key::Named(key::Named::F10) if modifiers.shift() => {
                self.publish(
                    shell,
                    Action::Menu {
                        path: entry.path.clone(),
                        position: row_bounds(bounds, index, state.scroll).center(),
                    },
                );
                None
            }
            _ => return false,
        };
        if let Some(next) = next {
            state.focus_path = self.entries.get(next).map(|entry| entry.path.clone());
            state.reveal(next, bounds.height, self.entries.len());
        }
        true
    }
}

#[derive(Default)]
struct State {
    root: Option<PathBuf>,
    focused: bool,
    focus_path: Option<PathBuf>,
    scroll: f32,
    pointer: Option<Point>,
    hovered: Option<PathBuf>,
    hover_since: Option<Instant>,
    press: Option<Press>,
    target: Option<PathBuf>,
    target_since: Option<Instant>,
    expansion_sent: bool,
}

struct Press {
    path: PathBuf,
    origin: Point,
    menu: bool,
    dragging: bool,
}

impl State {
    fn dragging(&self) -> Option<&Press> {
        self.press.as_ref().filter(|press| press.dragging)
    }
    fn cancel_drag(&mut self) {
        self.press = None;
        self.target = None;
        self.target_since = None;
        self.expansion_sent = false;
    }
    fn move_pointer(&mut self, point: Point) {
        self.pointer = Some(point);
        if let Some(press) = &mut self.press
            && !press.menu
            && self.root.as_deref() != Some(press.path.as_path())
            && press.origin.distance(point) >= DRAG_THRESHOLD
        {
            press.dragging = true;
        }
    }
    fn clamp_scroll(&mut self, height: f32, count: usize) {
        self.scroll = self
            .scroll
            .clamp(0.0, (count as f32 * ROW_HEIGHT - height).max(0.0));
    }
    fn reveal(&mut self, index: usize, height: f32, count: usize) {
        let top = index as f32 * ROW_HEIGHT;
        if top < self.scroll {
            self.scroll = top;
        } else if top + ROW_HEIGHT > self.scroll + height {
            self.scroll = top + ROW_HEIGHT - height;
        }
        self.clamp_scroll(height, count);
    }
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn focus(&mut self) {
        self.focused = true;
    }
    fn unfocus(&mut self) {
        self.focused = false;
        self.cancel_drag();
    }
}

impl<Message> Widget<Message, Theme, Renderer> for FileTree<'_, Message> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }
    fn diff(&self, tree: &mut Tree) {
        self.reconcile(tree.state.downcast_mut::<State>());
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State>();
        self.reconcile(state);
        let size = limits.width(Length::Fill).height(Length::Fill).max();
        state.clamp_scroll(size.height, self.entries.len());
        layout::Node::new(size)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        operation.focusable(
            Some(&widget::Id::from(ID)),
            layout.bounds(),
            tree.state.downcast_mut::<State>(),
        );
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        self.reconcile(state);
        let bounds = layout.bounds();
        let now = Instant::now();
        if !self.enabled {
            return;
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                state.move_pointer(*position);
                if state.dragging().is_some() {
                    self.hover_target(state, bounds, *viewport, now);
                    if let Some(since) = state.target_since {
                        shell.request_redraw_at(since + HOVER_DELAY);
                    }
                    shell.request_redraw_at(now + Duration::from_millis(16));
                    shell.capture_event();
                } else {
                    let hovered = self
                        .hovered(state, bounds, *viewport, *position)
                        .map(|index| self.entries[index].path.clone());
                    if state.hovered != hovered {
                        state.hovered = hovered;
                        state.hover_since = Some(now);
                        shell.request_redraw_at(now + HOVER_DELAY);
                    }
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(button)) => {
                let hovered = cursor
                    .position()
                    .and_then(|point| self.hovered(state, bounds, *viewport, point));
                if let Some(index) = hovered {
                    let entry = &self.entries[index];
                    let point = cursor.position().unwrap_or_else(|| bounds.center());
                    state.focused = true;
                    state.focus_path = Some(entry.path.clone());
                    self.publish(shell, Action::Focus);
                    match button {
                        mouse::Button::Right => self.publish(
                            shell,
                            Action::Menu {
                                path: entry.path.clone(),
                                position: point,
                            },
                        ),
                        mouse::Button::Left => {
                            state.press = Some(Press {
                                path: entry.path.clone(),
                                origin: point,
                                menu: menu_bounds(row_bounds(bounds, index, state.scroll))
                                    .contains(point),
                                dragging: false,
                            })
                        }
                        _ => return,
                    }
                    shell.capture_event();
                    shell.request_redraw();
                } else if state.dragging().is_none() {
                    state.focused = false;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.press.is_some() =>
            {
                let press = state.press.take().expect("press checked before release");
                if press.dragging {
                    if let Some(destination) = state.target.take()
                        && self.valid_destination(&press.path, &destination)
                    {
                        self.publish(shell, Action::Expand(destination.clone()));
                        self.publish(
                            shell,
                            Action::Move {
                                source: press.path,
                                destination,
                            },
                        );
                    }
                } else if cursor
                    .position()
                    .and_then(|point| self.hovered(state, bounds, *viewport, point))
                    .is_some_and(|index| self.entries[index].path == press.path)
                    && let Some(entry) = self.entries.iter().find(|entry| entry.path == press.path)
                {
                    if press.menu {
                        self.publish(
                            shell,
                            Action::Menu {
                                path: press.path,
                                position: cursor.position().unwrap_or(bounds.center()),
                            },
                        );
                    } else {
                        self.publish(
                            shell,
                            if entry.directory {
                                Action::Toggle(press.path)
                            } else {
                                Action::Open(press.path)
                            },
                        );
                    }
                }
                state.cancel_drag();
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if cursor.is_over(bounds) && cursor.is_over(*viewport) =>
            {
                let delta = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y * ROW_HEIGHT,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                state.scroll -= delta;
                state.clamp_scroll(bounds.height, self.entries.len());
                if state.dragging().is_some() {
                    self.hover_target(state, bounds, *viewport, now);
                }
                state.hover_since = Some(now);
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(key::Named::Escape),
                ..
            }) if state.press.is_some() => {
                state.cancel_drag();
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && self.keyboard(state, key, *modifiers, bounds, shell) =>
            {
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Window(window::Event::Unfocused) | Event::Mouse(mouse::Event::CursorLeft) => {
                state.cancel_drag();
                state.pointer = None;
                state.hovered = None;
                shell.request_redraw();
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(press) = state.dragging() {
                    let source = press.path.clone();
                    if let Some(point) = state.pointer {
                        let speed = edge_scroll(point, bounds);
                        if speed != 0.0 {
                            state.scroll += speed;
                            state.clamp_scroll(bounds.height, self.entries.len());
                            self.hover_target(state, bounds, *viewport, *now);
                        }
                        shell.request_redraw_at(*now + Duration::from_millis(16));
                    }
                    if !state.expansion_sent
                        && state.target_since.is_some_and(|since| {
                            now.saturating_duration_since(since) >= HOVER_DELAY
                        })
                        && let Some(target) = &state.target
                        && self.valid_destination(&source, target)
                        && self
                            .entries
                            .iter()
                            .any(|entry| entry.path == *target && !entry.expanded)
                    {
                        self.publish(shell, Action::Expand(target.clone()));
                        state.expansion_sent = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.start_layer(clip);
        let mut frame = Frame::new(renderer, bounds.size());
        let first = (state.scroll / ROW_HEIGHT).floor().max(0.0) as usize;
        let last = ((state.scroll + bounds.height) / ROW_HEIGHT)
            .ceil()
            .min(self.entries.len() as f32) as usize;
        for index in first..last {
            let entry = &self.entries[index];
            let row = row_bounds(bounds, index, state.scroll);
            let hovered = cursor.is_over(row) && cursor.is_over(clip);
            let focused =
                state.focused && state.focus_path.as_deref() == Some(entry.path.as_path());
            let selected = self.active == Some(entry.path.as_path());
            let dragging = state
                .dragging()
                .is_some_and(|press| press.path == entry.path);
            let target = state.dragging().is_some_and(|press| {
                state.target.as_deref() == Some(entry.path.as_path())
                    && self.valid_destination(&press.path, &entry.path)
            });
            let settled = if self.highlighted == Some(entry.path.as_path()) {
                1.0
            } else {
                0.0
            };
            let fill = styles::sidebar_fill(self.tokens, target || selected, hovered);
            let border = if target || focused {
                self.tokens.accent
            } else {
                self.tokens.accent.scale_alpha(settled)
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: row,
                    border: Border {
                        color: border,
                        width: if target || focused || settled > 0.0 {
                            1.0
                        } else {
                            0.0
                        },
                        radius: styles::SIDEBAR_RADIUS.into(),
                    },
                    ..renderer::Quad::default()
                },
                fill,
            );
            let alpha = if dragging || !self.enabled { 0.45 } else { 1.0 };
            let ink = if selected {
                self.tokens.on_accent_soft
            } else {
                self.tokens.muted
            }
            .scale_alpha(alpha);
            let muted = self.tokens.muted.scale_alpha(alpha);
            // Retain a readable filename even in a deeply nested folder. The
            // tooltip and context menu keep the complete filesystem location.
            let x = 7.0 + (entry.depth as f32 * INDENT).min((row.width - 170.0).max(0.0));
            draw_icon(
                &mut frame,
                Point::new(x + 14.0, row.center_y() - bounds.y - 7.0),
                entry.directory,
                muted,
            );
            if entry.directory {
                draw_chevron(
                    &mut frame,
                    Point::new(x + 3.0, row.center_y() - bounds.y),
                    entry.expanded,
                    muted,
                );
            }
            let label = Rectangle {
                x: bounds.x + x + 36.0,
                y: row.y,
                width: (row.width - x - 67.0).max(1.0),
                height: ROW_HEIGHT,
            };
            draw_label(
                renderer,
                &entry.label,
                label,
                styles::SIDEBAR_TEXT_SIZE,
                styles::BODY_FONT,
                ink,
                clip,
            );
            if state.dragging().is_none() && (hovered || focused) {
                let menu = menu_bounds(row);
                let color = if cursor.is_over(menu) {
                    self.tokens.accent
                } else {
                    self.tokens.muted
                };
                for offset in [-4.0, 0.0, 4.0] {
                    frame.fill(
                        &CanvasPath::circle(
                            Point::new(
                                menu.center_x() - bounds.x + offset,
                                menu.center_y() - bounds.y,
                            ),
                            1.0,
                        ),
                        color,
                    );
                }
            }
        }
        if let Some(press) = state.dragging()
            && state.target.as_deref() == self.root
            && let Some(root) = self.root
            && self.valid_destination(&press.path, root)
        {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: Border {
                        color: self.tokens.accent,
                        width: 1.0,
                        radius: styles::SIDEBAR_RADIUS.into(),
                    },
                    ..Default::default()
                },
                self.tokens.accent_soft.scale_alpha(0.15),
            );
        }
        renderer.with_translation(bounds.position() - Point::ORIGIN, |renderer| {
            renderer.draw_geometry(frame.into_geometry())
        });
        let content = self.entries.len() as f32 * ROW_HEIGHT;
        if content > bounds.height {
            let height = (bounds.height * bounds.height / content)
                .max(24.0)
                .min(bounds.height);
            let y = bounds.y + state.scroll / (content - bounds.height) * (bounds.height - height);
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle::new(
                        Point::new((bounds.x + bounds.width) - 3.0, y),
                        Size::new(2.0, height),
                    ),
                    border: Border {
                        radius: 1.0.into(),
                        ..Border::default()
                    },
                    ..renderer::Quad::default()
                },
                self.tokens.border,
            );
        }
        renderer.end_layer();
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if !self.enabled {
            return if cursor.is_over(layout.bounds()) && cursor.is_over(*viewport) {
                mouse::Interaction::Wait
            } else {
                mouse::Interaction::None
            };
        }
        if let Some(press) = state.dragging() {
            return if state
                .target
                .as_deref()
                .is_some_and(|target| self.valid_destination(&press.path, target))
            {
                mouse::Interaction::Grabbing
            } else {
                mouse::Interaction::NoDrop
            };
        }
        if cursor.is_over(layout.bounds()) && cursor.is_over(*viewport) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        _layout: Layout<'b>,
        _renderer: &Renderer,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if !self.enabled {
            return None;
        }
        let state = tree.state.downcast_ref::<State>();
        let pointer = state.pointer?;
        let (label, detail) = if let Some(press) = state.dragging() {
            let label = press.path.file_name().map_or_else(
                || press.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            let detail = match state.target.as_deref() {
                Some(target) if self.valid_destination(&press.path, target) => format!(
                    "Move to {}",
                    target
                        .file_name()
                        .unwrap_or(target.as_os_str())
                        .to_string_lossy()
                ),
                Some(target) if press.path.parent() == Some(target) => {
                    "Already in this folder".into()
                }
                Some(_) => "Cannot move here".into(),
                None => "Choose a folder".into(),
            };
            (label, Some(detail))
        } else if state
            .hover_since
            .is_some_and(|since| since.elapsed() >= HOVER_DELAY)
        {
            (state.hovered.as_ref()?.display().to_string(), None)
        } else {
            return None;
        };
        Some(overlay::Element::new(Box::new(Hint {
            label,
            detail,
            point: pointer + translation + Vector::new(14.0, 18.0),
            tokens: self.tokens,
        })))
    }
}

impl<'a, Message: 'a> From<FileTree<'a, Message>> for Element<'a, Message> {
    fn from(tree: FileTree<'a, Message>) -> Self {
        Element::new(tree)
    }
}

fn row_at(y: f32, scroll: f32, count: usize) -> Option<usize> {
    let index = ((y + scroll) / ROW_HEIGHT).floor();
    (y >= 0.0 && index >= 0.0 && (index as usize) < count).then_some(index as usize)
}
fn row_bounds(bounds: Rectangle, index: usize, scroll: f32) -> Rectangle {
    Rectangle {
        y: bounds.y + index as f32 * ROW_HEIGHT - scroll,
        height: ROW_HEIGHT,
        ..bounds
    }
}
fn menu_bounds(row: Rectangle) -> Rectangle {
    Rectangle {
        x: (row.x + row.width) - 28.0,
        width: 28.0,
        ..row
    }
}
fn valid_destination(
    source: &Path,
    destination: &Path,
    entries: &[Entry],
    root: Option<&Path>,
) -> bool {
    if root != Some(destination)
        && !entries
            .iter()
            .any(|entry| entry.path == destination && entry.directory)
    {
        return false;
    }
    if source == destination
        || destination.starts_with(source)
        || source.parent() == Some(destination)
    {
        return false;
    }
    let Some(name) = source.file_name() else {
        return false;
    };
    let target = destination.join(name);
    !entries
        .iter()
        .any(|entry| entry.path == target && entry.path != source)
}
fn edge_scroll(point: Point, bounds: Rectangle) -> f32 {
    if point.x < bounds.x
        || point.x > (bounds.x + bounds.width)
        || point.y < bounds.y - 24.0
        || point.y > (bounds.y + bounds.height) + 24.0
    {
        return 0.0;
    }
    if point.y < bounds.y + 28.0 {
        -((bounds.y + 28.0 - point.y) * 0.35).min(14.0)
    } else if point.y > (bounds.y + bounds.height) - 28.0 {
        ((point.y - ((bounds.y + bounds.height) - 28.0)) * 0.35).min(14.0)
    } else {
        0.0
    }
}

fn draw_label(
    renderer: &mut Renderer,
    label: &str,
    bounds: Rectangle,
    size: f32,
    font: Font,
    color: Color,
    clip: Rectangle,
) {
    let Some(clip) = bounds.intersection(&clip) else {
        return;
    };
    renderer.fill_text(
        text::Text {
            content: label.to_owned(),
            bounds: bounds.size(),
            size: size.into(),
            line_height: text::LineHeight::Relative(1.2),
            font,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Center,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
        },
        Point::new(bounds.x, bounds.center_y()),
        color,
        clip,
    );
}
fn draw_icon(frame: &mut Frame, point: Point, directory: bool, color: Color) {
    let path = CanvasPath::new(|p| {
        if directory {
            p.move_to(point + Vector::new(0.0, 3.0));
            p.line_to(point + Vector::new(5.0, 3.0));
            p.line_to(point + Vector::new(7.0, 5.0));
            p.line_to(point + Vector::new(14.0, 5.0));
            p.line_to(point + Vector::new(13.0, 13.0));
            p.line_to(point + Vector::new(0.0, 13.0));
            p.close();
            p.move_to(point + Vector::new(0.0, 3.0));
            p.line_to(point);
            p.line_to(point + Vector::new(5.0, 0.0));
            p.line_to(point + Vector::new(7.0, 2.0));
            p.line_to(point + Vector::new(13.0, 2.0));
        } else {
            p.move_to(point);
            p.line_to(point + Vector::new(7.0, 0.0));
            p.line_to(point + Vector::new(11.0, 4.0));
            p.line_to(point + Vector::new(11.0, 14.0));
            p.line_to(point + Vector::new(0.0, 14.0));
            p.close();
            p.move_to(point + Vector::new(7.0, 0.0));
            p.line_to(point + Vector::new(7.0, 4.0));
            p.line_to(point + Vector::new(11.0, 4.0));
            p.move_to(point + Vector::new(3.0, 8.0));
            p.line_to(point + Vector::new(8.0, 8.0));
            p.move_to(point + Vector::new(3.0, 11.0));
            p.line_to(point + Vector::new(8.0, 11.0));
        }
    });
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(color)
            .with_width(1.2)
            .with_line_cap(LineCap::Round),
    );
}
fn draw_chevron(frame: &mut Frame, point: Point, expanded: bool, color: Color) {
    let path = CanvasPath::new(|p| {
        if expanded {
            p.move_to(point + Vector::new(-2.0, -1.0));
            p.line_to(point + Vector::new(1.0, 2.0));
            p.line_to(point + Vector::new(4.0, -1.0));
        } else {
            p.move_to(point + Vector::new(0.0, -3.0));
            p.line_to(point + Vector::new(3.0, 0.0));
            p.line_to(point + Vector::new(0.0, 3.0));
        }
    });
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(color)
            .with_width(1.2)
            .with_line_cap(LineCap::Round),
    );
}

struct Hint {
    label: String,
    detail: Option<String>,
    point: Point,
    tokens: Tokens,
}
impl<Message> overlay::Overlay<Message, Theme, Renderer> for Hint {
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        let max_width = (if self.detail.is_some() {
            250.0_f32
        } else {
            520.0
        })
        .min((bounds.width - 16.0).max(1.0));
        let paragraph = <Renderer as text::Renderer>::Paragraph::with_text(text::Text {
            content: self.label.as_str(),
            bounds: Size::new((max_width - 20.0).max(1.0), 180.0),
            size: 11.0.into(),
            line_height: text::LineHeight::Relative(1.4),
            font: Font::MONOSPACE,
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::WordOrGlyph,
        });
        let width = (paragraph.min_width() + 20.0).min(max_width);
        let height = (paragraph.min_height() + if self.detail.is_some() { 37.0 } else { 18.0 })
            .min((bounds.height - 16.0).max(1.0));
        layout::Node::new(Size::new(width, height)).move_to(Point::new(
            self.point
                .x
                .clamp(8.0, (bounds.width - width - 8.0).max(8.0)),
            self.point
                .y
                .clamp(8.0, (bounds.height - height - 8.0).max(8.0)),
        ))
    }
    fn draw(
        &self,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    color: self.tokens.border,
                    width: 1.0,
                    radius: 6.0.into(),
                },
                shadow: Shadow {
                    color: self.tokens.shadow,
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 12.0,
                },
                ..renderer::Quad::default()
            },
            self.tokens.elevated,
        );
        renderer.fill_text(
            text::Text {
                content: self.label.clone(),
                bounds: Size::new(
                    (bounds.width - 20.0).max(1.0),
                    (bounds.height - 16.0).max(1.0),
                ),
                size: 11.0.into(),
                font: Font::MONOSPACE,
                line_height: text::LineHeight::Relative(1.4),
                align_x: text::Alignment::Default,
                align_y: iced::alignment::Vertical::Top,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::WordOrGlyph,
            },
            bounds.position() + Vector::new(10.0, 8.0),
            self.tokens.text,
            bounds,
        );
        if let Some(detail) = &self.detail {
            draw_label(
                renderer,
                detail,
                Rectangle {
                    x: bounds.x + 10.0,
                    y: (bounds.y + bounds.height) - 25.0,
                    width: bounds.width - 20.0,
                    height: 18.0,
                },
                11.0,
                Font::DEFAULT,
                self.tokens.muted,
                bounds,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, directory: bool) -> Entry {
        Entry {
            path: path.into(),
            label: path.into(),
            depth: 1,
            directory,
            expanded: false,
        }
    }

    #[test]
    fn renamed_inactive_row_keeps_keyboard_focus_on_the_result() {
        let entries = vec![
            entry("/notes", true),
            entry("/notes/Active.md", false),
            entry("/notes/Renamed.md", false),
        ];
        let tree = FileTree::new(
            &entries,
            Some(Path::new("/notes/Active.md")),
            crate::theme::Preset::Paper.resolve().tokens,
            |action| action,
        )
        .highlighted(Some(Path::new("/notes/Renamed.md")));
        let mut state = State {
            root: Some("/notes".into()),
            focus_path: Some("/notes/Before.md".into()),
            hovered: Some("/notes/Before.md".into()),
            hover_since: Some(Instant::now() - HOVER_DELAY),
            focused: true,
            ..State::default()
        };
        tree.reconcile(&mut state);
        assert_eq!(
            state.focus_path.as_deref(),
            Some(Path::new("/notes/Renamed.md"))
        );
        assert!(state.focused);
        assert!(state.hovered.is_none());
        assert!(state.hover_since.is_none());
    }

    #[test]
    fn drops_reject_cycles_collisions_and_existing_parent() {
        let entries = vec![
            entry("/notes", true),
            entry("/notes/Essay.md", false),
            entry("/notes/Drafts", true),
            entry("/notes/Drafts/Essay.md", false),
        ];
        assert!(!valid_destination(
            Path::new("/notes/Drafts"),
            Path::new("/notes/Drafts/Inside"),
            &entries,
            None
        ));
        assert!(!valid_destination(
            Path::new("/notes/Essay.md"),
            Path::new("/notes"),
            &entries,
            None
        ));
        assert!(!valid_destination(
            Path::new("/notes/Essay.md"),
            Path::new("/notes/Drafts"),
            &entries,
            None
        ));
        assert!(valid_destination(
            Path::new("/notes/Draft"),
            Path::new("/notes/Drafts"),
            &entries,
            None
        ));
    }

    #[test]
    fn workspace_background_accepts_moves_without_a_duplicate_root_row() {
        let entries = vec![
            entry("/notes/Drafts", true),
            entry("/notes/Drafts/Essay.md", false),
        ];
        assert!(valid_destination(
            Path::new("/notes/Drafts/Essay.md"),
            Path::new("/notes"),
            &entries,
            Some(Path::new("/notes"))
        ));
        assert!(!valid_destination(
            Path::new("/notes/Drafts"),
            Path::new("/notes/Drafts"),
            &entries,
            Some(Path::new("/notes"))
        ));
    }
    #[test]
    fn pointer_threshold_and_cancel_do_not_become_file_activation() {
        let mut state = State {
            root: Some("/notes".into()),
            press: Some(Press {
                path: "/notes/Essay.md".into(),
                origin: Point::new(10.0, 10.0),
                menu: false,
                dragging: false,
            }),
            ..State::default()
        };
        state.move_pointer(Point::new(13.0, 12.0));
        assert!(state.dragging().is_none());
        state.move_pointer(Point::new(20.0, 12.0));
        assert!(state.dragging().is_some());
        state.cancel_drag();
        assert!(state.press.is_none());
        assert!(state.target.is_none());
    }
    #[test]
    fn row_geometry_and_keyboard_reveal_use_the_same_scroll_coordinates() {
        let mut state = State::default();
        state.reveal(18, 180.0, 30);
        assert_eq!(row_at(179.0, state.scroll, 30), Some(18));
        assert_eq!(
            row_bounds(
                Rectangle::new(Point::new(8.0, 20.0), Size::new(240.0, 180.0)),
                18,
                state.scroll
            )
            .y + ROW_HEIGHT,
            200.0
        );
        state.reveal(0, 180.0, 30);
        assert_eq!(state.scroll, 0.0);
        assert_eq!(row_at(-1.0, 0.0, 30), None);
    }
    #[test]
    fn root_and_actions_area_never_start_a_move() {
        for (path, menu) in [("/notes", false), ("/notes/Essay.md", true)] {
            let mut state = State {
                root: Some("/notes".into()),
                press: Some(Press {
                    path: path.into(),
                    origin: Point::ORIGIN,
                    menu,
                    dragging: false,
                }),
                ..State::default()
            };
            state.move_pointer(Point::new(100.0, 100.0));
            assert!(state.dragging().is_none());
        }
    }
}
