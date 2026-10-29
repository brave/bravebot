//! The reading of the machine's records, off the thread that reads keys (SCREEN-17).
//!
//! The first character typed in a search reads every record and the index of what each says, which
//! takes as long as parsing them once does. The periodic read of the list is the same reader. Both
//! run here, one request at a time in the order they were sent, and the screen draws and takes keys
//! while they run. What comes back is applied by the thread that owns the screen, which asks for
//! it on each pass of its loop, so a result is never applied from two places.

use bravebot_session::jobs::Roster;
use bravebot_session::listing::{self, Ask, Current, Page, Reader};
use bravebot_session::search::{Corpus, Query};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

/// A read of the list: what `Reader::read` is given, owned so that it can cross threads.
pub struct PageAsk {
    pub root: PathBuf,
    /// The session the terminal is in, and whether it is working.
    pub current: Option<(String, bool)>,
    pub wanted: usize,
    pub archived: HashMap<String, u64>,
}

/// A search of what the listed sessions say.
pub struct SaidAsk {
    pub query: Query,
    /// The id and directory of each session listed.
    pub rows: Vec<(String, String)>,
}

enum Request {
    Page(PageAsk),
    Said(SaidAsk),
}

/// What a request found.
pub enum Reply {
    Page(Page),
    Said {
        /// The text it was found for, so that a late answer to text that has since changed is seen
        /// to be one.
        phrase: String,
        lines: HashMap<String, String>,
    },
}

/// The thread that reads, and the two ends of the conversation with it.
pub struct Worker {
    requests: Sender<Request>,
    replies: Receiver<Reply>,
    pages: usize,
    said: usize,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Worker")
            .field("pages", &self.pages)
            .field("said", &self.said)
            .finish()
    }
}

type ReadPage = Box<dyn FnMut(PageAsk) -> Page + Send>;
type ReadSaid = Box<dyn FnMut(SaidAsk) -> HashMap<String, String> + Send>;

impl Worker {
    /// A worker that reads the records on disk, asking `open` whether another process holds one.
    pub fn start(open: fn(&Path, &str) -> bool) -> Self {
        let mut reader = Reader::new();
        let mut cache: HashMap<String, (HashSet<String>, Corpus)> = HashMap::new();
        Self::start_with(
            Box::new(move |ask| read_page(&mut reader, open, &ask)),
            Box::new(move |ask| read_said(&mut cache, &ask)),
        )
    }

    /// A worker that answers with these, for a test that holds a read open or counts the requests.
    pub fn start_with(mut read_page: ReadPage, mut read_said: ReadSaid) -> Self {
        let (requests, asked) = channel::<Request>();
        let (answered, replies) = channel::<Reply>();
        std::thread::spawn(move || {
            while let Ok(request) = asked.recv() {
                let reply = match request {
                    Request::Page(ask) => Reply::Page(read_page(ask)),
                    Request::Said(ask) => {
                        let phrase = ask.query.phrase().to_string();
                        Reply::Said {
                            phrase,
                            lines: read_said(ask),
                        }
                    }
                };
                if answered.send(reply).is_err() {
                    return;
                }
            }
        });
        Self {
            requests,
            replies,
            pages: 0,
            said: 0,
        }
    }

    /// Ask for the list, unless a read of it is already under way.
    pub fn ask_page(&mut self, ask: PageAsk) -> bool {
        if self.pages > 0 || self.requests.send(Request::Page(ask)).is_err() {
            return false;
        }
        self.pages += 1;
        true
    }

    /// Ask what the listed sessions say.
    pub fn ask_said(&mut self, ask: SaidAsk) {
        if self.requests.send(Request::Said(ask)).is_ok() {
            self.said += 1;
        }
    }

    /// The next answer that has arrived, if one has.
    pub fn answer(&mut self) -> Option<Reply> {
        let reply = self.replies.try_recv().ok()?;
        match &reply {
            Reply::Page(_) => self.pages = self.pages.saturating_sub(1),
            Reply::Said { .. } => self.said = self.said.saturating_sub(1),
        }
        Some(reply)
    }

    /// Whether a request has not been answered yet.
    pub fn busy(&self) -> bool {
        self.pages + self.said > 0
    }

    /// How many requests for the list and for what was said have been sent and not answered.
    #[cfg(test)]
    pub fn outstanding(&self) -> (usize, usize) {
        (self.pages, self.said)
    }
}

fn read_page(reader: &mut Reader, open: fn(&Path, &str) -> bool, ask: &PageAsk) -> Page {
    let roster = Roster::readable();
    let current = ask.current.as_ref().map(|(id, working)| Current {
        id: id.as_str(),
        working: *working,
    });
    let listed = Ask {
        root: &ask.root,
        roster: roster.as_ref(),
        current,
        wanted: ask.wanted,
    };
    reader.read(&listed, open, |id, updated| {
        listing::is_archived(&ask.archived, id, updated)
    })
}

/// The first line of each listed session's words that holds the phrase. A directory's index is read
/// again only when a session of it has arrived that the last read was not asked for.
fn read_said(
    cache: &mut HashMap<String, (HashSet<String>, Corpus)>,
    ask: &SaidAsk,
) -> HashMap<String, String> {
    let mut found = HashMap::new();
    if !ask.query.has_phrase() {
        return found;
    }
    let mut by_directory: HashMap<&str, HashSet<String>> = HashMap::new();
    for (id, directory) in &ask.rows {
        by_directory
            .entry(directory.as_str())
            .or_default()
            .insert(id.clone());
    }
    for (directory, ids) in by_directory {
        let current = cache
            .get(directory)
            .is_some_and(|(read, _)| ids.is_subset(read));
        if !current {
            let corpus = Corpus::read(Path::new(directory), |id| ids.contains(id));
            cache.insert(directory.to_string(), (ids.clone(), corpus));
        }
        if let Some((_, corpus)) = cache.get(directory) {
            for id in &ids {
                if let Some(line) = corpus.found(id, &ask.query) {
                    found.insert(id.clone(), line);
                }
            }
        }
    }
    found
}
