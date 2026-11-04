//! A small native accessibility bridge for applications supplying their own tree.
//!
//! This is deliberately separate from Iced's widget tree: the application owns
//! the semantics and handles actions using its ordinary update path. Every
//! published update must contain a complete tree, including its [`Tree`].
//! Node geometry uses logical window-client coordinates. The bridge scales the
//! root transform to native physical pixels, including on display changes.
//!
//! [`Tree`]: accesskit::Tree

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use std::sync::{Arc, Mutex};

/// Shared accessibility state for one application window.
///
/// Pass a clone to [`crate::run_with_accessibility`] and publish subsequent
/// complete trees through [`Self::update`]. Action callbacks must enqueue work
/// for the application's update path rather than mutating application state
/// reentrantly. They may be called from a platform accessibility callback.
#[derive(Clone)]
pub struct Config {
    snapshot: Arc<Mutex<Snapshot>>,
    handler: Arc<dyn Fn(ActionRequest) + Send + Sync>,
}

#[derive(Clone)]
struct Snapshot {
    tree: Arc<TreeUpdate>,
    generation: u64,
}

impl Config {
    /// Creates a bridge with the complete initial accessibility tree.
    pub fn new(
        initial_tree: TreeUpdate,
        handler: impl Fn(ActionRequest) + Send + Sync + 'static,
    ) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Snapshot {
                tree: Arc::new(initial_tree),
                generation: 0,
            })),
            handler: Arc::new(handler),
        }
    }

    /// Replaces the complete tree that native accessibility clients observe.
    ///
    /// The runtime applies changed snapshots after processing application
    /// messages. If accessibility is activated later, its initial tree is this
    /// latest snapshot rather than the tree originally passed to [`Self::new`].
    pub fn update(&self, tree: TreeUpdate) {
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        snapshot.tree = Arc::new(tree);
        snapshot.generation = snapshot.generation.wrapping_add(1);
    }

    fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

struct Activation {
    config: Config,
    scale: Arc<Mutex<f64>>,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let scale = *self
            .scale
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(physical_tree(&self.config.snapshot().tree, scale))
    }
}

fn physical_tree(tree: &TreeUpdate, scale: f64) -> TreeUpdate {
    let mut update = tree.clone();
    if let Some(root) = update.tree.as_ref().map(|tree| tree.root)
        && let Some((_, node)) =
            update.nodes.iter_mut().find(|(id, _)| *id == root)
    {
        let transform = node
            .transform()
            .copied()
            .unwrap_or(accesskit::Affine::IDENTITY);
        node.set_transform(accesskit::Affine::scale(scale) * transform);
    }
    update
}

struct Actions(Arc<dyn Fn(ActionRequest) + Send + Sync>);

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        (self.0)(request);
    }
}

struct Deactivation;

impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

/// Keeps the adapter alive until the runtime closes its native window.
///
/// The adapter is dropped before our last retained window reference, so the
/// native view remains valid while AccessKit restores its original class.
pub(crate) struct Window {
    adapter: accesskit_winit::Adapter,
    raw: Arc<winit::window::Window>,
    config: Config,
    generation: u64,
    scale: Arc<Mutex<f64>>,
    applied_scale: f64,
}

impl Window {
    pub(crate) fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        window: Arc<winit::window::Window>,
        config: Config,
    ) -> Self {
        let generation = config.snapshot().generation;
        let applied_scale = window.scale_factor();
        let scale = Arc::new(Mutex::new(applied_scale));
        let adapter = accesskit_winit::Adapter::with_direct_handlers(
            event_loop,
            &window,
            Activation {
                config: config.clone(),
                scale: scale.clone(),
            },
            Actions(config.handler.clone()),
            Deactivation,
        );

        Self {
            adapter,
            raw: window,
            config,
            generation,
            scale,
            applied_scale,
        }
    }

    pub(crate) fn process_event(&mut self, event: &winit::event::WindowEvent) {
        if let winit::event::WindowEvent::ScaleFactorChanged {
            scale_factor,
            ..
        } = event
        {
            *self
                .scale
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                *scale_factor;
        }
        self.adapter.process_event(&self.raw, event);
    }

    pub(crate) fn refresh(&mut self) {
        let snapshot = self.config.snapshot();
        let scale = *self
            .scale
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if snapshot.generation != self.generation || scale != self.applied_scale
        {
            self.adapter
                .update_if_active(|| physical_tree(&snapshot.tree, scale));
            self.generation = snapshot.generation;
            self.applied_scale = scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Affine, Node, NodeId, Point, Role, Tree};

    fn tree(label: &str) -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_label(label);
        root.set_transform(Affine::translate((3.0, 4.0)));
        TreeUpdate {
            nodes: vec![(NodeId(0), root)],
            tree: Some(Tree::new(NodeId(0))),
            focus: NodeId(0),
        }
    }

    #[test]
    fn native_scaling_preserves_logical_snapshot() {
        let logical = tree("Document");
        let physical = physical_tree(&logical, 2.0);
        let root = &physical.nodes[0].1;
        assert_eq!(
            *root.transform().unwrap() * Point::new(1.0, 2.0),
            Point::new(8.0, 12.0),
        );
        assert_eq!(
            *logical.nodes[0].1.transform().unwrap() * Point::new(1.0, 2.0),
            Point::new(4.0, 6.0),
        );
        assert_eq!(physical_tree(&logical, 2.0), physical);
    }

    #[test]
    fn delayed_activation_reads_latest_tree_and_scale() {
        let config = Config::new(tree("Initial"), |_| {});
        let scale = Arc::new(Mutex::new(1.0));
        let mut activation = Activation {
            config: config.clone(),
            scale: scale.clone(),
        };
        config.update(tree("Changed"));
        *scale.lock().unwrap() = 2.0;
        assert_eq!(
            activation.request_initial_tree().unwrap(),
            physical_tree(&tree("Changed"), 2.0),
        );
    }
}
