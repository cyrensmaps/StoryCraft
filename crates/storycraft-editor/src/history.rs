//! Snapshot-based undo/redo.
//!
//! The UI mutates the project directly. Each frame the app compares the
//! project with a copy taken before the UI ran; the first change starts a
//! pending edit, and the edit is committed once the user has let go of the
//! pointer and paused briefly. A whole drag, or a burst of typing, therefore
//! becomes a single undo step.

use storycraft_model::Project;

const MAX_STEPS: usize = 200;
/// Seconds of inactivity that end an edit.
const SETTLE_SECONDS: f64 = 0.5;

#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
    /// State before the edit currently in progress.
    pending: Option<Project>,
    last_change: f64,
}

impl History {
    /// Record the outcome of one UI frame. Returns true if the project changed.
    pub fn observe(&mut self, before: Project, now: &Project, pointer_down: bool, time: f64) -> bool {
        if &before != now {
            if self.pending.is_none() {
                self.pending = Some(before);
            }
            self.last_change = time;
            self.redo.clear();
            true
        } else {
            if !pointer_down && time - self.last_change > SETTLE_SECONDS {
                self.commit();
            }
            false
        }
    }

    /// Whether an uncommitted edit is waiting to settle (the UI should keep
    /// repainting until it does).
    pub fn is_settling(&self) -> bool {
        self.pending.is_some()
    }

    /// Finish the edit in progress so it becomes its own undo step.
    pub fn commit(&mut self) {
        if let Some(p) = self.pending.take() {
            self.undo.push(p);
            if self.undo.len() > MAX_STEPS {
                self.undo.remove(0);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        self.pending.is_some() || !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, current: &mut Project) {
        self.commit();
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous));
        }
    }

    pub fn redo(&mut self, current: &mut Project) {
        self.commit();
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next));
        }
    }

    pub fn clear(&mut self) {
        *self = History::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_and_undoes() {
        let mut h = History::default();
        let mut p = Project::new("a");
        let v0 = p.clone();

        // A drag: several changing frames with the pointer down.
        for (i, t) in [0.0, 0.1, 0.2].into_iter().enumerate() {
            let before = p.clone();
            p.title = format!("a{i}");
            h.observe(before, &p, true, t);
        }
        // Settles after release.
        h.observe(p.clone(), &p, false, 1.0);
        assert!(!h.is_settling());

        h.undo(&mut p);
        assert_eq!(p, v0, "the whole drag is one step");
        h.redo(&mut p);
        assert_eq!(p.title, "a2");
    }

    #[test]
    fn undo_while_pending_reverts_the_pending_edit() {
        let mut h = History::default();
        let mut p = Project::new("a");
        let before = p.clone();
        p.title = "b".into();
        h.observe(before, &p, false, 0.0);
        h.undo(&mut p);
        assert_eq!(p.title, "a");
    }
}
