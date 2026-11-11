//! Asks the disk whether the shelf's files are still there, off the UI thread. On a
//! network drive that went to sleep every ask waits for the network; drawing the rows
//! froze the shelf and the settings window when it asked on each frame.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use shelve_core::Availability;

/// The checker's answers by path. A path not in it yet counts as present.
pub type Presence = HashMap<PathBuf, Availability>;

pub fn is_missing(presence: &Presence, path: &Path) -> bool {
    presence.get(path) == Some(&Availability::Missing)
}

/// Paths sent and not yet taken by the checker.
type Waiting = Arc<Mutex<HashSet<PathBuf>>>;

/// The asking end, kept by the shelf.
pub struct Queue {
    tx: Sender<PathBuf>,
    waiting: Waiting,
}

/// The answering end; runs on its own long-lived thread.
pub struct Checker {
    rx: Receiver<PathBuf>,
    waiting: Waiting,
}

pub fn queue() -> (Queue, Checker) {
    let (tx, rx) = mpsc::channel();
    let waiting = Waiting::default();
    let checker = Checker {
        rx,
        waiting: waiting.clone(),
    };
    (Queue { tx, waiting }, checker)
}

impl Queue {
    /// Asks about `path`. One that is still waiting is not queued twice, so asks do
    /// not pile up behind a dead network drive.
    pub fn check(&self, path: PathBuf) {
        if self.waiting.lock().unwrap().insert(path.clone()) {
            let _ = self.tx.send(path);
        }
    }
}

impl Checker {
    /// Answers the paths one by one until the queue is dropped.
    pub fn run(self, answer: impl Fn(PathBuf, Availability)) {
        let Checker { rx, waiting } = self;
        for path in rx {
            waiting.lock().unwrap().remove(&path);
            let a = if path.exists() {
                Availability::Present
            } else {
                Availability::Missing
            };
            answer(path, a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn answers(queue: Queue, checker: Checker) -> Vec<(PathBuf, Availability)> {
        drop(queue); // the checker stops once the queue is gone and empty
        let (tx, rx) = mpsc::channel();
        checker.run(move |path, a| tx.send((path, a)).unwrap());
        rx.try_iter().collect()
    }

    #[test]
    fn two_requests_for_one_path_get_one_answer() {
        // A dead network drive answers slowly: repeated asks must not pile up behind it.
        let (queue, checker) = queue();
        let path = std::env::temp_dir();
        queue.check(path.clone());
        queue.check(path.clone());
        assert_eq!(answers(queue, checker), vec![(path, Availability::Present)]);
    }

    #[test]
    fn a_path_answered_can_be_asked_again() {
        let (queue, checker) = queue();
        let gone = PathBuf::from(r"C:\shelve-нет\a.txt");
        queue.check(gone.clone());
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || checker.run(move |p, a| tx.send((p, a)).unwrap()));
        assert_eq!(rx.recv().unwrap(), (gone.clone(), Availability::Missing));
        queue.check(gone.clone());
        assert_eq!(rx.recv().unwrap(), (gone, Availability::Missing));
        drop(queue);
        worker.join().unwrap();
    }
}
