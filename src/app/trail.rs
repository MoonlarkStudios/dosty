//! The documents visited in this window, in order, for Back and Forward.

use std::path::{Path, PathBuf};

use super::session::{Session, SessionId};
use crate::io::workspace_document::HostedAddress;

const MAX_STOPS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Back,
    Forward,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Destination {
    File(PathBuf),
    Hosted(HostedAddress),
}

/// One visited document. Unsaved sessions live in the background and saved
/// documents can be reopened at their destination even after eviction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stop {
    pub(super) session: SessionId,
    pub(super) destination: Option<Destination>,
}

impl Stop {
    fn is(&self, arrival: &Self) -> bool {
        self.session == arrival.session
            || (self.destination.is_some() && self.destination == arrival.destination)
    }
}

#[derive(Default)]
pub(super) struct Trail {
    stops: Vec<Stop>,
    /// Index of the document on the page. Meaningless while `stops` is empty.
    position: usize,
    /// The stop a Back or Forward step is travelling to. Opening may be
    /// asynchronous or fail, so `position` moves only when that stop arrives.
    heading: Option<usize>,
}

impl Trail {
    /// Records that a document took the page.
    ///
    /// Arriving where a step was heading moves along the trail. Any other
    /// arrival is a new visit and replaces everything forward of the page.
    pub(super) fn visit(&mut self, session: &Session) {
        let stop = Stop {
            session: session.id(),
            destination: session.path().cloned().map(Destination::File).or_else(|| {
                session
                    .hosted()
                    .map(|draft| Destination::Hosted(draft.address.clone()))
            }),
        };
        let arrived = self
            .heading
            .take()
            .into_iter()
            .chain([self.position])
            .find(|&index| {
                self.stops
                    .get(index)
                    .is_some_and(|previous| previous.is(&stop))
            });
        if let Some(index) = arrived {
            self.stops[index] = stop;
            self.position = index;
            return;
        }
        self.stops.truncate(self.position + 1);
        self.stops.push(stop);
        if self.stops.len() > MAX_STOPS {
            self.stops.remove(0);
        }
        self.position = self.stops.len() - 1;
    }

    fn target(&self, direction: Direction) -> Option<usize> {
        match direction {
            Direction::Back => self.position.checked_sub(1),
            Direction::Forward => Some(self.position + 1),
        }
        .filter(|&index| index < self.stops.len())
    }

    pub(super) fn can_step(&self, direction: Direction) -> bool {
        self.target(direction).is_some()
    }

    /// Starts a step and returns the stop to open. The page moves only when
    /// `visit` reports that stop arriving.
    pub(super) fn step(&mut self, direction: Direction) -> Option<Stop> {
        let target = self.target(direction)?;
        self.heading = Some(target);
        Some(self.stops[target].clone())
    }

    pub(super) fn cancel_step(&mut self) {
        self.heading = None;
    }

    /// Removes a session without a destination to reopen it from.
    pub(super) fn forget(&mut self, session: SessionId) {
        let mut index = 0;
        while index < self.stops.len() {
            if self.stops[index].session != session || self.stops[index].destination.is_some() {
                index += 1;
                continue;
            }
            self.stops.remove(index);
            if index <= self.position {
                self.position = self.position.saturating_sub(1);
            }
            self.heading = match self.heading {
                Some(heading) if heading == index => None,
                Some(heading) if heading > index => Some(heading - 1),
                heading => heading,
            };
        }
    }

    /// Follows files moved from `from` to `to`, including everything below a
    /// moved folder.
    pub(super) fn moved(&mut self, from: &Path, to: &Path) {
        for stop in &mut self.stops {
            if let Some(Destination::File(path)) = &mut stop.destination
                && let Ok(relative) = path.strip_prefix(from)
            {
                *path = if relative.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(relative)
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Destination, Direction, Trail};
    use crate::app::application::session::Session;
    use crate::document::Document;

    fn session(path: Option<&str>) -> Session {
        Session::new(
            Document::new(String::new()),
            path.map(Into::into),
            "Test.md".into(),
        )
    }

    fn visit(trail: &mut Trail, session: &Session) {
        trail.visit(session);
    }

    #[test]
    fn steps_move_only_when_the_stop_arrives_and_new_visits_drop_forward_stops() {
        let (first, second, third) = (
            session(Some("/one.md")),
            session(None),
            session(Some("/three.md")),
        );
        let mut trail = Trail::default();
        assert!(!trail.can_step(Direction::Back));
        for document in [&first, &second, &third] {
            visit(&mut trail, document);
        }
        assert!(!trail.can_step(Direction::Forward));

        let stop = trail.step(Direction::Back).unwrap();
        assert_eq!(stop.session, second.id());
        assert!(
            !trail.can_step(Direction::Forward),
            "a step that has not arrived leaves the page where it was"
        );
        visit(&mut trail, &second);
        assert!(trail.can_step(Direction::Forward));

        // A reloaded file is a new session at the same path.
        trail.step(Direction::Back).unwrap();
        let reloaded = session(Some("/one.md"));
        visit(&mut trail, &reloaded);
        assert!(!trail.can_step(Direction::Back));
        assert_eq!(trail.step(Direction::Forward).unwrap().session, second.id());

        // Abandoning that step for somewhere new replaces the forward stops.
        let fourth = session(Some("/four.md"));
        visit(&mut trail, &fourth);
        assert!(!trail.can_step(Direction::Forward));
        assert_eq!(trail.step(Direction::Back).unwrap().session, reloaded.id());
    }

    #[test]
    fn revisiting_the_page_in_place_adds_no_stop() {
        let mut document = session(None);
        let mut trail = Trail::default();
        visit(&mut trail, &document);
        document.assume_saved_file("/saved.md".into());
        visit(&mut trail, &document);
        assert!(!trail.can_step(Direction::Back));
    }

    #[test]
    fn dropped_drafts_are_forgotten_and_moved_files_are_followed() {
        let (first, draft, third) = (
            session(Some("/notes/one.md")),
            session(None),
            session(Some("/notes/deep/three.md")),
        );
        let mut trail = Trail::default();
        for document in [&first, &draft, &third] {
            visit(&mut trail, document);
        }
        trail.forget(draft.id());
        trail.forget(first.id());
        trail.moved(Path::new("/notes"), Path::new("/archive"));

        let stop = trail.step(Direction::Back).unwrap();
        assert_eq!(
            stop.destination,
            Some(Destination::File("/archive/one.md".into()))
        );
        visit(&mut trail, &first);
        let stop = trail.step(Direction::Forward).unwrap();
        assert_eq!(
            stop.destination,
            Some(Destination::File("/archive/deep/three.md".into()))
        );
    }

    #[test]
    fn the_trail_is_bounded() {
        let mut trail = Trail::default();
        for _ in 0..super::MAX_STOPS + 5 {
            visit(&mut trail, &session(None));
        }
        assert_eq!(trail.stops.len(), super::MAX_STOPS);
        assert_eq!(trail.position, super::MAX_STOPS - 1);
    }
}
