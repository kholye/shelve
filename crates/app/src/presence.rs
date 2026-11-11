//! Asks the disk whether the shelf's files are still there, off the UI thread. On a
//! network drive that went to sleep every ask waits for the network; drawing the rows
//! froze the shelf and the settings window when it asked on each frame.
//!
//! After a file is dropped somewhere it is watched for a while: Explorer often moves it
//! only after the drop is over (to another drive, from a network drive, behind its "file
//! already exists" dialog). Once it is gone the row leaves the shelf.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use shelve_core::Availability;

/// How often a watched file is looked at.
const WATCH_EVERY: Duration = Duration::from_millis(500);
/// How long after a drop a file is watched. A copy never goes away; this lets it go.
const WATCH_FOR: Duration = Duration::from_secs(30);

/// The checker's answers by path. A path not in it yet counts as present.
pub type Presence = HashMap<PathBuf, Availability>;

pub fn is_missing(presence: &Presence, path: &Path) -> bool {
    presence.get(path) == Some(&Availability::Missing)
}

/// What the checker found about one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub path: PathBuf,
    pub availability: Availability,
    /// Found while the path was watched after a drop.
    pub after_drop: bool,
}

/// The row leaves the shelf only if its file went away while watched after a drop: then
/// Explorer moved it. Gone at any other time, the row is greyed instead. Shelve never
/// deletes a file itself.
pub fn remove_after_drop(watched: bool, availability: Availability) -> bool {
    watched && availability == Availability::Missing
}

enum Ask {
    Check(PathBuf),
    Watch(PathBuf),
}

/// Paths sent and not yet taken by the checker.
type Waiting = Arc<Mutex<HashSet<PathBuf>>>;

/// The asking end, kept by the shelf.
pub struct Queue {
    tx: Sender<Ask>,
    waiting: Waiting,
}

/// The answering end; runs on its own long-lived thread.
pub struct Checker {
    rx: Receiver<Ask>,
    waiting: Waiting,
    every: Duration,
    limit: Duration,
}

pub fn queue() -> (Queue, Checker) {
    queue_timed(WATCH_EVERY, WATCH_FOR)
}

/// A queue whose watches look every `every` for at most `limit`.
fn queue_timed(every: Duration, limit: Duration) -> (Queue, Checker) {
    let (tx, rx) = mpsc::channel();
    let waiting = Waiting::default();
    let checker = Checker {
        rx,
        waiting: waiting.clone(),
        every,
        limit,
    };
    (Queue { tx, waiting }, checker)
}

impl Queue {
    /// Asks about `path`. One that is still waiting is not queued twice, so asks do
    /// not pile up behind a dead network drive.
    pub fn check(&self, path: PathBuf) {
        if self.waiting.lock().unwrap().insert(path.clone()) {
            let _ = self.tx.send(Ask::Check(path));
        }
    }

    /// Watches `path` after a drop. The only answer is "gone, after a drop"; a file that
    /// stays gives none. Watching it again starts the time over.
    pub fn watch(&self, path: PathBuf) {
        let _ = self.tx.send(Ask::Watch(path));
    }
}

fn availability(path: &Path) -> Availability {
    if path.exists() {
        Availability::Present
    } else {
        Availability::Missing
    }
}

impl Checker {
    /// Answers until the queue is dropped. Sleeps without a timeout while nothing is
    /// watched, so an idle shelf costs no processor time.
    pub fn run(self, answer: impl Fn(Answer)) {
        let Checker {
            rx,
            waiting,
            every,
            limit,
        } = self;
        // Watched paths and when their watch ends.
        let mut watched: HashMap<PathBuf, Instant> = HashMap::new();
        let mut next_look = Instant::now();
        loop {
            let ask = if watched.is_empty() {
                match rx.recv() {
                    Ok(ask) => Some(ask),
                    Err(_) => return,
                }
            } else {
                match rx.recv_timeout(next_look.saturating_duration_since(Instant::now())) {
                    Ok(ask) => Some(ask),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            };
            match ask {
                Some(Ask::Check(path)) => {
                    waiting.lock().unwrap().remove(&path);
                    answer(Answer {
                        availability: availability(&path),
                        path,
                        after_drop: false,
                    });
                }
                Some(Ask::Watch(path)) => {
                    if watched.is_empty() {
                        next_look = Instant::now();
                    }
                    watched.insert(path, Instant::now() + limit);
                }
                None => {}
            }
            let now = Instant::now();
            if !watched.is_empty() && now >= next_look {
                watched.retain(|path, until| {
                    if path.exists() {
                        return now < *until;
                    }
                    answer(Answer {
                        path: path.clone(),
                        availability: Availability::Missing,
                        after_drop: true,
                    });
                    false
                });
                next_look = now + every;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread::{sleep, JoinHandle};

    fn answers(queue: Queue, checker: Checker) -> Vec<Answer> {
        drop(queue); // the checker stops once the queue is gone and empty
        let (tx, rx) = mpsc::channel();
        checker.run(move |a| tx.send(a).unwrap());
        rx.try_iter().collect()
    }

    fn answer(path: &Path, availability: Availability, after_drop: bool) -> Answer {
        Answer {
            path: path.to_path_buf(),
            availability,
            after_drop,
        }
    }

    /// Runs the checker on its own thread; its answers come out of the receiver.
    fn spawn(checker: Checker) -> (Receiver<Answer>, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || checker.run(move |a| tx.send(a).unwrap()));
        (rx, worker)
    }

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shelve-{}-presence", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        p
    }

    const SOON: Duration = Duration::from_secs(5);

    #[test]
    fn two_requests_for_one_path_get_one_answer() {
        // A dead network drive answers slowly: repeated asks must not pile up behind it.
        let (queue, checker) = queue();
        let path = std::env::temp_dir();
        queue.check(path.clone());
        queue.check(path.clone());
        assert_eq!(
            answers(queue, checker),
            vec![answer(&path, Availability::Present, false)]
        );
    }

    #[test]
    fn a_path_answered_can_be_asked_again() {
        let (queue, checker) = queue();
        let gone = PathBuf::from(r"C:\shelve-нет\a.txt");
        queue.check(gone.clone());
        let (rx, worker) = spawn(checker);
        assert_eq!(
            rx.recv().unwrap(),
            answer(&gone, Availability::Missing, false)
        );
        queue.check(gone.clone());
        assert_eq!(
            rx.recv().unwrap(),
            answer(&gone, Availability::Missing, false)
        );
        drop(queue);
        worker.join().unwrap();
    }

    #[test]
    fn a_watched_file_that_goes_away_later_is_reported_as_gone_after_a_drop() {
        // Explorer often moves the file after DoDragDrop has returned: to another drive,
        // from a network drive, behind its "file already exists" dialog.
        let file = temp_file("переехал.txt");
        let (queue, checker) = queue_timed(Duration::from_millis(20), SOON);
        queue.watch(file.clone());
        let (rx, worker) = spawn(checker);
        sleep(Duration::from_millis(100));
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            rx.recv_timeout(SOON).unwrap(),
            answer(&file, Availability::Missing, true)
        );
        drop(queue);
        worker.join().unwrap();
    }

    #[test]
    fn a_watched_file_that_stays_gives_no_answer_and_is_let_go() {
        // A copy: the file never leaves. After the limit it is no longer watched, so
        // deleting it later does not take the row off the shelf.
        let file = temp_file("копия.txt");
        let (queue, checker) = queue_timed(Duration::from_millis(10), Duration::from_millis(100));
        queue.watch(file.clone());
        let (rx, worker) = spawn(checker);
        sleep(Duration::from_millis(400));
        std::fs::remove_file(&file).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        drop(queue);
        worker.join().unwrap();
    }

    #[test]
    fn watching_again_starts_the_limit_over() {
        let file = temp_file("снова.txt");
        let (queue, checker) = queue_timed(Duration::from_millis(10), Duration::from_millis(400));
        queue.watch(file.clone());
        let (rx, worker) = spawn(checker);
        sleep(Duration::from_millis(300));
        queue.watch(file.clone());
        sleep(Duration::from_millis(300));
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            rx.recv_timeout(SOON).unwrap(),
            answer(&file, Availability::Missing, true)
        );
        drop(queue);
        worker.join().unwrap();
    }

    #[test]
    fn only_a_file_gone_after_a_drop_leaves_the_shelf() {
        assert!(remove_after_drop(true, Availability::Missing));
        assert!(!remove_after_drop(true, Availability::Present));
        // Gone outside a watch: the row is greyed, as before.
        assert!(!remove_after_drop(false, Availability::Missing));
        assert!(!remove_after_drop(false, Availability::Present));
    }
}
