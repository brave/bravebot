//! Keeping a session's vectors caught up with its record, off the turn ([RETRIEVE-4]).
//!
//! A pass reads what the sidecar already holds and embeds what the record has that it does not,
//! a batch at a time, so a pass cut short leaves every row before it standing and the next pass
//! carries on from there ([RETRIEVE-20]). Nothing a pass needs is kept between passes: what is
//! owed is always worked out again from the record, which is why a job the queue had no room for
//! costs nothing but time.
//!
//! [RETRIEVE-4]: ../../../docs/specs/retrieval.md#RETRIEVE-4
//! [RETRIEVE-20]: ../../../docs/specs/retrieval.md#RETRIEVE-20

use crate::audit::Trail;
use crate::vectors::{self, Index, Model, Opened, Row, Speaker, Writer};
use bravebot_agent::conversation::Snapshot;
use bravebot_aichat::embeddings::{Embedder, Failure, Kind, MAX_BATCH, SharedSink};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

/// How many passes may wait for the one running. A session saves once a turn, so a queue this
/// long is full only when the endpoint is far slower than the conversation.
pub const QUEUE: usize = 8;

/// One chunk the record says the sidecar should hold.
#[derive(Debug)]
struct Owed<'a> {
    ordinal: u32,
    chunk: u16,
    speaker: Speaker,
    range: Range<usize>,
    text: &'a str,
}

impl Owed<'_> {
    fn words(&self) -> &str {
        self.text.get(self.range.clone()).unwrap_or_default()
    }

    fn is(&self, row: &Row) -> bool {
        row.ordinal == self.ordinal
            && row.chunk == self.chunk
            && row.speaker == self.speaker
            && row.range.start as usize == self.range.start
            && row.range.end as usize == self.range.end
            && row.digest == vectors::digest(self.words())
    }
}

/// Every chunk the record says the sidecar should hold, in the order it is written.
fn owed(snapshot: &Snapshot) -> Vec<Owed<'_>> {
    vectors::indexable(snapshot)
        .into_iter()
        .flat_map(|passage| {
            vectors::chunks(passage.text)
                .into_iter()
                .zip(0..=u16::MAX)
                .map(move |(range, chunk)| Owed {
                    ordinal: passage.ordinal,
                    chunk,
                    speaker: passage.speaker,
                    range,
                    text: passage.text,
                })
        })
        .collect()
}

/// Whether the sidecar's rows are where the record says they belong, as far as they go. A message
/// put in the place of one a turn dropped before its record was cut fails this.
fn agrees(index: &Index, owed: &[Owed<'_>]) -> bool {
    index.rows.len() <= owed.len() && index.rows.iter().zip(owed).all(|(row, owed)| owed.is(row))
}

fn model_of(embedder: &dyn Embedder) -> Option<Model> {
    Some(Model {
        endpoint: embedder.endpoint(),
        id: embedder.model().to_string(),
        dimensions: u32::try_from(embedder.dimensions()).ok()?,
    })
}

/// How a pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    /// The sidecar holds every chunk the record owes it.
    CaughtUp {
        /// How many rows this pass added.
        written: usize,
    },
    /// The endpoint or the file failed. The rows written before are kept, and the next pass
    /// carries on from them.
    Failed {
        /// How many rows this pass added before it failed.
        written: usize,
        /// Why the endpoint gave no vectors, where it was the endpoint and not the file.
        why: Option<Failure>,
    },
    /// The sidecar was cut or removed while the pass ran, so the record it read is not the one
    /// the file is kept for any more, and nothing more was written.
    Abandoned,
    /// The settings choose no endpoint to embed with, so the sidecar was neither read nor touched:
    /// rebuilding it for no model would lose the vectors of the one it was built with.
    Unconfigured,
    /// Retrieval was off as the pass started or before one of its batches, so nothing more was
    /// sent. The rows written before are kept.
    Off {
        /// How many rows this pass added before retrieval went off.
        written: usize,
    },
}

/// Bring the sidecar at `path` up to `snapshot`, embedding with `embedder`.
///
/// `generation` is the sidecar's [`vectors::generation`] when `snapshot` was taken. A sidecar that
/// was cut or removed since is left as it is. One that is missing, stale, or holds rows the record
/// does not agree with is started again; otherwise the pass appends to what is there. A record
/// with nothing to embed and no sidecar writes none.
pub fn catch_up(
    path: &Path,
    snapshot: &Snapshot,
    embedder: &dyn Embedder,
    generation: u64,
) -> Pass {
    catch_up_while(path, snapshot, embedder, generation, &|| true)
}

/// [`catch_up`], asking `on` as the pass starts and before each later batch is sent, and sending
/// nothing more once it answers false ([RETRIEVE-4]).
///
/// [RETRIEVE-4]: ../../../docs/specs/retrieval.md#RETRIEVE-4
pub fn catch_up_while(
    path: &Path,
    snapshot: &Snapshot,
    embedder: &dyn Embedder,
    generation: u64,
    on: &dyn Fn() -> bool,
) -> Pass {
    if !on() {
        return Pass::Off { written: 0 };
    }
    let Some(model) = model_of(embedder) else {
        return Pass::Failed {
            written: 0,
            why: None,
        };
    };
    let owed = owed(snapshot);
    let opened = vectors::open(path, &model, vectors::record_length(snapshot));
    let kept = match &opened {
        Opened::Current(index) if agrees(index, &owed) => Some(index),
        _ => None,
    };
    let from = kept.map_or(0, |index| index.rows.len());
    if from == owed.len() && (kept.is_some() || opened == Opened::Missing) {
        return Pass::CaughtUp { written: 0 };
    }
    let started = vectors::unless_changed(path, generation, || match kept {
        Some(index) => Writer::resume(path, index),
        None => Writer::create(path, &model),
    });
    let mut writer = match started {
        None => return Pass::Abandoned,
        Some(Err(_)) => {
            return Pass::Failed {
                written: 0,
                why: None,
            };
        }
        Some(Ok(writer)) => writer,
    };

    let mut written = 0;
    for (at, batch) in owed[from..].chunks(MAX_BATCH).enumerate() {
        if at > 0 && !on() {
            return Pass::Off { written };
        }
        let texts: Vec<String> = batch.iter().map(|owed| owed.words().to_string()).collect();
        let vectors = match embedder.embed(Kind::Passage, &texts) {
            Ok(vectors)
                if vectors.len() == batch.len()
                    && vectors
                        .iter()
                        .all(|vector| vector.len() == model.dimensions as usize) =>
            {
                vectors
            }
            Ok(_) => {
                return Pass::Failed {
                    written,
                    why: Some(Failure::Failed),
                };
            }
            Err(error) => {
                return Pass::Failed {
                    written,
                    why: Some(error.failure()),
                };
            }
        };
        let appended = vectors::unless_changed(path, generation, || {
            for (owed, vector) in batch.iter().zip(vectors) {
                writer.append(&Row {
                    ordinal: owed.ordinal,
                    chunk: owed.chunk,
                    speaker: owed.speaker,
                    range: offset(owed.range.start)?..offset(owed.range.end)?,
                    digest: vectors::digest(owed.words()),
                    vector,
                })?;
                written += 1;
            }
            std::io::Result::Ok(())
        });
        match appended {
            None => return Pass::Abandoned,
            Some(Err(_)) => {
                return Pass::Failed { written, why: None };
            }
            Some(Ok(())) => {}
        }
    }
    Pass::CaughtUp { written }
}

/// A byte offset as a row holds it. A message past 4 GiB fails its pass instead of naming wrong words.
fn offset(at: usize) -> std::io::Result<u32> {
    u32::try_from(at).map_err(std::io::Error::other)
}

/// Whether the sidecar at `path` holds every chunk `snapshot` owes it of the messages before
/// ordinal `before`, from `model`, so a turn may retrieve from them ([RETRIEVE-21]).
///
/// [RETRIEVE-21]: ../../../docs/specs/retrieval.md#RETRIEVE-21
pub fn ready(path: &Path, snapshot: &Snapshot, model: &Model, before: u32) -> bool {
    let owed = owed(snapshot);
    let needed = owed.iter().take_while(|owed| owed.ordinal < before).count();
    if needed == 0 {
        return true;
    }
    match vectors::open(path, model, vectors::record_length(snapshot)) {
        Opened::Current(index) => agrees(&index, &owed) && index.rows.len() >= needed,
        _ => false,
    }
}

/// How far the sidecar at a path has caught up with its record, in messages, for a person to read
/// ([RETRIEVE-29](../../../docs/specs/retrieval.md#RETRIEVE-29)). Counts only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    /// Where the sidecar stands.
    pub state: Standing,
    /// How many of the messages the record owes the sidecar it holds every chunk of.
    pub indexed: usize,
    /// How many messages the record owes the sidecar.
    pub eligible: usize,
}

/// Where a sidecar stands against its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// It holds every chunk the record owes it.
    Ready,
    /// It is behind, and the next pass carries on from what it holds.
    Indexing,
    /// What it holds is from another model or chunker, unreadable, or not the record's, so it is
    /// built again from the start.
    Rebuilding,
    /// The conversation met untrusted content, so nothing of it is embedded.
    Untrusted,
}

/// How far the sidecar at `path` holds what `snapshot` owes it from `model`.
pub fn readiness(path: &Path, snapshot: &Snapshot, model: &Model) -> Readiness {
    let owed = owed(snapshot);
    let messages = |owed: &[Owed<'_>]| {
        owed.iter()
            .map(|owed| owed.ordinal)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    };
    if snapshot.context != "trusted" {
        return Readiness {
            state: Standing::Untrusted,
            indexed: 0,
            eligible: 0,
        };
    }
    let eligible = messages(&owed);
    let held = match vectors::open(path, model, vectors::record_length(snapshot)) {
        Opened::Current(index) if agrees(&index, &owed) => index.rows.len(),
        Opened::Missing => 0,
        Opened::Current(_) | Opened::Rebuild(_) if !owed.is_empty() => {
            return Readiness {
                state: Standing::Rebuilding,
                indexed: 0,
                eligible,
            };
        }
        Opened::Current(_) | Opened::Rebuild(_) => 0,
    };
    // A message is held once its last chunk is.
    let held = held.min(owed.len());
    let split = held > 0
        && owed
            .get(held)
            .is_some_and(|next| next.ordinal == owed[held - 1].ordinal);
    let whole = messages(&owed[..held]) - usize::from(split);
    Readiness {
        state: if held >= owed.len() {
            Standing::Ready
        } else {
            Standing::Indexing
        },
        indexed: whole,
        eligible,
    }
}

/// One pass to run: a session's record as a turn saved it, and where its vectors and its trail
/// are.
#[derive(Debug)]
pub struct Job {
    path: PathBuf,
    directory: PathBuf,
    id: String,
    turn: usize,
    snapshot: Snapshot,
    generation: u64,
}

impl Job {
    /// The pass for session `id`, whose record is in `directory`, up to `snapshot`, which turn
    /// `turn` saved. Taken with the sidecar's generation now, so a cut after it abandons the
    /// pass.
    pub(crate) fn new(directory: &Path, id: &str, snapshot: Snapshot, turn: usize) -> Self {
        let path = vectors::path_of(directory, id);
        Self {
            generation: vectors::generation(&path),
            path,
            directory: directory.to_path_buf(),
            id: id.to_string(),
            turn,
            snapshot,
        }
    }
}

/// Builds the embedder for one pass, writing what its requests' gates decided to the trail it is
/// given, or none where the settings choose no endpoint.
pub type Embedders = Box<dyn Fn(SharedSink) -> Option<Box<dyn Embedder>> + Send>;

/// Whether retrieval is on now, asked as a pass starts and before each batch it sends after the
/// first, so a pass queued before the setting went off sends nothing once it has.
pub type Switch = Box<dyn Fn() -> bool + Send>;

/// Run `job` with an embedder from `embedders` while `on` says retrieval is on, and write its
/// requests' trail to the session's, under the turn that offered it.
pub fn run(embedders: &Embedders, on: &dyn Fn() -> bool, job: &Job) -> Pass {
    run_against(embedders, on, job).0
}

/// [`run`], with the endpoint the pass sent to where it had one.
fn run_against(embedders: &Embedders, on: &dyn Fn() -> bool, job: &Job) -> (Pass, Option<String>) {
    if !on() {
        return (Pass::Off { written: 0 }, None);
    }
    let trail = Arc::new(Mutex::new(Trail::new()));
    let Some(embedder) = embedders(trail.clone()) else {
        return (Pass::Unconfigured, None);
    };
    let pass = catch_up_while(&job.path, &job.snapshot, &*embedder, job.generation, on);
    let trail = trail.lock().unwrap_or_else(PoisonError::into_inner);
    if !trail.events().is_empty() {
        crate::sessions::append_audit_in(&job.directory, &job.id, job.turn, trail.events());
    }
    (pass, Some(embedder.endpoint()))
}

/// Why each sidecar's last pass got no vectors from its endpoint, kept until a pass makes progress
/// so the window can say what is wrong rather than that indexing goes on ([RETRIEVE-29]).
///
/// [RETRIEVE-29]: ../../../docs/specs/retrieval.md#RETRIEVE-29
#[derive(Debug, Clone, Default)]
struct Failures(Arc<Mutex<HashMap<PathBuf, (String, Failure)>>>);

impl Failures {
    fn note(&self, path: &Path, endpoint: &str, pass: Pass) {
        let mut failures = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match pass {
            Pass::Failed {
                written: 0,
                why: Some(why),
            } => {
                failures.insert(path.to_path_buf(), (endpoint.to_string(), why));
            }
            Pass::CaughtUp { .. }
            | Pass::Failed { written: 1.., .. }
            | Pass::Off { written: 1.. } => {
                failures.remove(path);
            }
            Pass::Failed {
                written: 0,
                why: None,
            }
            | Pass::Abandoned
            | Pass::Unconfigured
            | Pass::Off { written: 0 } => {}
        }
    }

    fn of(&self, path: &Path, endpoint: &str) -> Option<Failure> {
        let failures = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        failures
            .get(path)
            .filter(|(sent_to, _)| sent_to == endpoint)
            .map(|&(_, why)| why)
    }
}

/// The queue passes wait in, and the one thread that runs them in turn.
#[derive(Debug, Clone)]
pub struct Indexer {
    jobs: SyncSender<Job>,
    pending: Arc<(Mutex<usize>, Condvar)>,
    failures: Failures,
}

impl Indexer {
    /// Start the thread, which runs each job with an embedder from `embedders`.
    pub fn start(embedders: Embedders) -> Self {
        Self::start_while(embedders, Box::new(|| true))
    }

    /// [`Indexer::start`], with each pass sending nothing once `on` says retrieval is off.
    pub fn start_while(embedders: Embedders, on: Switch) -> Self {
        let (jobs, queue) = sync_channel(QUEUE);
        let pending = Arc::new((Mutex::new(0), Condvar::new()));
        let counted = pending.clone();
        let failures = Failures::default();
        let noted = failures.clone();
        // A thread that cannot start drops the queue's receiver, so every offer is refused.
        std::thread::Builder::new()
            .name("vectors".to_string())
            .spawn(move || work(&embedders, &*on, &queue, &counted, &noted))
            .ok();
        Self {
            jobs,
            pending,
            failures,
        }
    }

    /// Queue `job` without waiting. False when the queue is full and the job was dropped: the
    /// next pass embeds what it would have from the record.
    pub fn offer(&self, job: Job) -> bool {
        let (count, _) = &*self.pending;
        let mut count = count.lock().unwrap_or_else(PoisonError::into_inner);
        let queued = self.jobs.try_send(job).is_ok();
        if queued {
            *count += 1;
        }
        queued
    }

    /// Why the last pass for the sidecar at `path` got no vectors from `endpoint`, as an
    /// [`endpoint_identity`](bravebot_aichat::embeddings::endpoint_identity) names it. None once a
    /// pass has made progress, or where the last failure was another endpoint's.
    pub fn failure(&self, path: &Path, endpoint: &str) -> Option<Failure> {
        self.failures.of(path, endpoint)
    }

    /// Wait until every queued pass has run.
    pub fn settle(&self) {
        let (count, finished) = &*self.pending;
        let count = count.lock().unwrap_or_else(PoisonError::into_inner);
        let _idle = finished
            .wait_while(count, |count| *count > 0)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

fn work(
    embedders: &Embedders,
    on: &dyn Fn() -> bool,
    queue: &Receiver<Job>,
    pending: &(Mutex<usize>, Condvar),
    failures: &Failures,
) {
    for job in queue {
        if let (pass, Some(endpoint)) = run_against(embedders, on, &job) {
            failures.note(&job.path, &endpoint, pass);
        }
        let (count, finished) = pending;
        let mut count = count.lock().unwrap_or_else(PoisonError::into_inner);
        *count = count.saturating_sub(1);
        finished.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_agent::conversation::Conversation;
    use bravebot_aichat::embeddings::{EmbedError, FakeEmbedder};
    use bravebot_aichat::protocol::Message;
    use bravebot_core::event::Event;
    use bravebot_core::label::Label;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const DIMENSIONS: usize = 4;

    fn scratch() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        (directory, path)
    }

    fn fake() -> FakeEmbedder {
        FakeEmbedder::new(DIMENSIONS)
    }

    fn model() -> Model {
        model_of(&fake()).expect("a model")
    }

    fn said(lines: &[&str]) -> Conversation {
        let mut conversation = Conversation::new();
        for (at, line) in lines.iter().enumerate() {
            conversation.push(if at % 2 == 0 {
                Message::user(*line)
            } else {
                Message::assistant(*line)
            });
        }
        conversation
    }

    fn rows_of(path: &Path, snapshot: &Snapshot) -> Vec<Row> {
        match vectors::open(path, &model(), vectors::record_length(snapshot)) {
            Opened::Current(index) => index.rows,
            other => panic!("the sidecar was not current: {other:?}"),
        }
    }

    fn places(rows: &[Row]) -> Vec<(u32, u16)> {
        rows.iter().map(|row| (row.ordinal, row.chunk)).collect()
    }

    /// Fails every call from the `fails_from`th on, counting from zero.
    struct Faltering {
        calls: AtomicUsize,
        fails_from: usize,
    }

    impl Embedder for Faltering {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            let failing =
                (self.calls.fetch_add(1, Ordering::SeqCst) >= self.fails_from).then_some(503);
            FakeEmbedder { failing, ..fake() }.embed(kind, texts)
        }
        fn endpoint(&self) -> String {
            fake().endpoint()
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn dimensions(&self) -> usize {
            DIMENSIONS
        }
    }

    /// RETRIEVE-20: a pass embeds each chunk the record owes and no more, and the next pass
    /// embeds only what the record gained since.
    #[test]
    fn a_pass_embeds_what_the_record_owes_and_the_next_carries_on_from_it() {
        let (_directory, path) = scratch();
        let mut conversation = said(&["the release train", "it leaves on thursday"]);
        let first = conversation.snapshot();

        assert_eq!(
            catch_up(&path, &first, &fake(), 0),
            Pass::CaughtUp { written: 2 }
        );
        conversation.push(Message::user("and the hotfix?"));
        let second = conversation.snapshot();
        assert_eq!(
            catch_up(&path, &second, &fake(), 0),
            Pass::CaughtUp { written: 1 }
        );
        assert_eq!(
            catch_up(&path, &second, &fake(), 0),
            Pass::CaughtUp { written: 0 }
        );
        assert_eq!(
            places(&rows_of(&path, &second)),
            vec![(0, 0), (1, 0), (2, 0)]
        );
    }

    /// RETRIEVE-20: an endpoint failing partway leaves the batches before it in the sidecar, which
    /// still reads as current, and the next pass embeds only the rest.
    #[test]
    fn a_failed_pass_keeps_its_rows_and_the_next_finishes_them() {
        let (_directory, path) = scratch();
        let lines: Vec<String> = (0..MAX_BATCH + 5)
            .map(|at| format!("message number {at}"))
            .collect();
        let snapshot = said(&lines.iter().map(String::as_str).collect::<Vec<_>>()).snapshot();
        let faltering = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: 1,
        };

        assert_eq!(
            catch_up(&path, &snapshot, &faltering, 0),
            Pass::Failed {
                written: MAX_BATCH,
                why: Some(Failure::Failed)
            }
        );
        assert_eq!(rows_of(&path, &snapshot).len(), MAX_BATCH);
        assert_eq!(
            catch_up(&path, &snapshot, &fake(), 0),
            Pass::CaughtUp { written: 5 }
        );
        assert_eq!(rows_of(&path, &snapshot).len(), MAX_BATCH + 5);
    }

    /// RETRIEVE-20: rows the record no longer agrees with, such as a message put in the place of
    /// one that went, and a sidecar from another model, are both started again from the record.
    #[test]
    fn a_sidecar_the_record_disagrees_with_is_built_again() {
        let (_directory, path) = scratch();
        let snapshot = said(&["first", "second"]).snapshot();
        let mut writer = Writer::create(&path, &model()).expect("create");
        writer
            .append(&Row {
                ordinal: 0,
                chunk: 0,
                speaker: Speaker::Planner,
                range: 0..5,
                digest: vectors::digest("first"),
                vector: vec![1.0; DIMENSIONS],
            })
            .expect("append");
        drop(writer);

        assert_eq!(
            catch_up(&path, &snapshot, &fake(), 0),
            Pass::CaughtUp { written: 2 }
        );
        let rows = rows_of(&path, &snapshot);
        assert_eq!(rows[0].speaker, Speaker::Person);

        let other = Model {
            id: "another-model".to_string(),
            ..model()
        };
        Writer::create(&path, &other).expect("create");
        assert_eq!(
            catch_up(&path, &snapshot, &fake(), 0),
            Pass::CaughtUp { written: 2 }
        );
        assert_eq!(places(&rows_of(&path, &snapshot)), vec![(0, 0), (1, 0)]);
    }

    /// RETRIEVE-20: a message replaced by another of the same length and speaker, whose rows
    /// would sit at the same places, is embedded again rather than kept under the old one's
    /// vector, and the vectors are not ready for it until it is.
    #[test]
    fn a_message_replaced_by_one_of_the_same_length_is_embedded_again() {
        let (_directory, path) = scratch();
        let before = said(&["deploy on monday", "noted"]).snapshot();
        assert_eq!(
            catch_up(&path, &before, &fake(), 0),
            Pass::CaughtUp { written: 2 }
        );
        let after = said(&["deploy on friday", "noted"]).snapshot();
        assert!(!ready(&path, &after, &model(), 2));
        assert_eq!(
            readiness(&path, &after, &model()).state,
            Standing::Rebuilding
        );

        let counted = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: usize::MAX,
        };
        assert_eq!(
            catch_up(&path, &after, &counted, 0),
            Pass::CaughtUp { written: 2 }
        );
        let rows = rows_of(&path, &after);
        assert_eq!(rows[0].digest, vectors::digest("deploy on friday"));
        assert_eq!(
            rows[0].vector,
            fake()
                .embed(Kind::Passage, &["deploy on friday".to_string()])
                .expect("a vector")[0]
        );
        assert!(ready(&path, &after, &model(), 2));
    }

    /// RETRIEVE-20: a pass over a record read before a rewind cut the sidecar, or before the
    /// session was deleted, writes nothing, and leaves no file where the session's went.
    #[test]
    fn a_pass_over_a_record_read_before_a_cut_writes_nothing() {
        let (_directory, path) = scratch();
        let snapshot = said(&["one", "two", "three"]).snapshot();
        let before = vectors::generation(&path);
        catch_up(&path, &snapshot, &fake(), before);

        let read = vectors::generation(&path);
        vectors::truncate(&path, 1).expect("truncate");
        assert_eq!(catch_up(&path, &snapshot, &fake(), read), Pass::Abandoned);
        assert_eq!(places(&rows_of(&path, &snapshot)), vec![(0, 0)]);

        let read = vectors::generation(&path);
        vectors::remove(&path).expect("remove");
        assert_eq!(catch_up(&path, &snapshot, &fake(), read), Pass::Abandoned);
        assert!(!path.exists(), "a deleted session's vectors came back");
    }

    fn lines(count: usize) -> Snapshot {
        let lines: Vec<String> = (0..count)
            .map(|at| format!("message number {at}"))
            .collect();
        said(&lines.iter().map(String::as_str).collect::<Vec<_>>()).snapshot()
    }

    /// Embeds as the fake does, except that the `wrong`th vector of the second call is a length
    /// other than the model's.
    struct Misshapen {
        calls: AtomicUsize,
        wrong: usize,
    }

    impl Embedder for Misshapen {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            let mut vectors = fake().embed(kind, texts)?;
            if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                vectors[self.wrong].push(0.5);
            }
            Ok(vectors)
        }
        fn endpoint(&self) -> String {
            fake().endpoint()
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn dimensions(&self) -> usize {
            DIMENSIONS
        }
    }

    /// RETRIEVE-15, RETRIEVE-20: a batch holding a vector of the wrong length fails the pass and
    /// no vector of that batch is written, wherever in it the wrong one is, and the batch before
    /// it is kept.
    #[test]
    fn a_wrong_length_vector_in_a_later_batch_fails_the_pass_and_keeps_the_batch_before() {
        for wrong in [0, 3] {
            let (_directory, path) = scratch();
            let snapshot = lines(MAX_BATCH + 5);
            let misshapen = Misshapen {
                calls: AtomicUsize::new(0),
                wrong,
            };
            assert_eq!(
                catch_up(&path, &snapshot, &misshapen, 0),
                Pass::Failed {
                    written: MAX_BATCH,
                    why: Some(Failure::Failed)
                },
                "the vector at {wrong} of the second batch"
            );
            assert_eq!(rows_of(&path, &snapshot).len(), MAX_BATCH);
            assert_eq!(
                catch_up(&path, &snapshot, &fake(), 0),
                Pass::CaughtUp { written: 5 }
            );
        }
    }

    /// Embeds as the fake does, holding every call from the `from`th on until its gate opens.
    struct HeldFrom {
        gate: Gate,
        calls: AtomicUsize,
        from: usize,
    }

    impl Embedder for HeldFrom {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            if self.calls.fetch_add(1, Ordering::SeqCst) >= self.from {
                self.gate.hold();
            }
            fake().embed(kind, texts)
        }
        fn endpoint(&self) -> String {
            fake().endpoint()
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn dimensions(&self) -> usize {
            DIMENSIONS
        }
    }

    /// RETRIEVE-20: a rewind's cut or the session's deletion while a pass waits on its second
    /// batch abandons the pass, which writes nothing more, and leaves no file where a deleted
    /// session's went.
    #[test]
    fn a_cut_or_a_removal_between_batches_abandons_the_pass() {
        for removed in [false, true] {
            let (_directory, path) = scratch();
            let snapshot = lines(MAX_BATCH + 5);
            let gate = Gate::default();
            let held = HeldFrom {
                gate: gate.clone(),
                calls: AtomicUsize::new(0),
                from: 1,
            };
            let generation = vectors::generation(&path);
            let pass = std::thread::scope(|scope| {
                let running = scope.spawn(|| catch_up(&path, &snapshot, &held, generation));
                gate.holding();
                assert_eq!(rows_of(&path, &snapshot).len(), MAX_BATCH);
                if removed {
                    vectors::remove(&path).expect("remove");
                } else {
                    vectors::truncate(&path, 1).expect("truncate");
                }
                gate.open();
                running.join().expect("the pass")
            });
            assert_eq!(pass, Pass::Abandoned);
            if removed {
                assert!(!path.exists(), "a deleted session's vectors came back");
            } else {
                assert_eq!(places(&rows_of(&path, &snapshot)), vec![(0, 0)]);
            }
        }
    }

    /// RETRIEVE-4: retrieval going off before a pass starts sends nothing and writes no sidecar,
    /// and going off between its batches sends nothing more and keeps the batches before.
    #[test]
    fn a_pass_sends_nothing_once_retrieval_is_off() {
        let (_directory, path) = scratch();
        let snapshot = lines(MAX_BATCH + 5);
        let counted = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: usize::MAX,
        };
        assert_eq!(
            catch_up_while(&path, &snapshot, &counted, 0, &|| false),
            Pass::Off { written: 0 }
        );
        assert_eq!(counted.calls.load(Ordering::SeqCst), 0);
        assert!(!path.exists());

        let asked = AtomicUsize::new(0);
        let on_for_one_batch = || asked.fetch_add(1, Ordering::SeqCst) == 0;
        assert_eq!(
            catch_up_while(&path, &snapshot, &counted, 0, &on_for_one_batch),
            Pass::Off { written: MAX_BATCH }
        );
        assert_eq!(counted.calls.load(Ordering::SeqCst), 1);
        assert_eq!(rows_of(&path, &snapshot).len(), MAX_BATCH);
    }

    /// RETRIEVE-4: a pass queued before retrieval went off asks for no embedder and sends nothing
    /// when its turn comes.
    #[test]
    fn a_pass_queued_before_retrieval_went_off_sends_nothing() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        let gate = Gate::default();
        let held = gate.clone();
        let built = Arc::new(AtomicUsize::new(0));
        let counted = built.clone();
        let on = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let switch = on.clone();
        let indexer = Indexer::start_while(
            Box::new(move |_| {
                counted.fetch_add(1, Ordering::SeqCst);
                Some(Box::new(Held(held.clone())))
            }),
            Box::new(move || switch.load(Ordering::SeqCst)),
        );
        let job = |lines: &[&str]| Job::new(directory.path(), "a1b2", said(lines).snapshot(), 1);

        assert!(indexer.offer(job(&["one"])));
        gate.holding();
        assert!(indexer.offer(job(&["one", "two"])));
        on.store(false, Ordering::SeqCst);
        gate.open();
        indexer.settle();

        assert_eq!(built.load(Ordering::SeqCst), 1);
        assert_eq!(
            places(&rows_of(&path, &said(&["one", "two"]).snapshot())),
            vec![(0, 0)]
        );
    }

    /// RETRIEVE-20: rows another process appended to the same sidecar, as a second copy of the
    /// program writing the same session would, are never read as the record's: the file is either
    /// unreadable or disagrees with the record, and the next pass builds it again whole.
    #[test]
    fn a_second_writer_appending_only_causes_a_rebuild() {
        let row = |ordinal: u32, text: &str| Row {
            ordinal,
            chunk: 0,
            speaker: if ordinal.is_multiple_of(2) {
                Speaker::Person
            } else {
                Speaker::Planner
            },
            range: 0..text.len() as u32,
            digest: vectors::digest(text),
            vector: fake()
                .embed(Kind::Passage, &[text.to_string()])
                .expect("a vector")
                .remove(0),
        };
        let seconds = [
            vec![row(2, "three")],
            vec![row(2, "drei")],
            vec![row(3, "four")],
        ];
        for second in seconds {
            let (_directory, path) = scratch();
            let snapshot = said(&["one", "two", "three"]).snapshot();
            let first = said(&["one", "two"]).snapshot();
            assert_eq!(
                catch_up(&path, &first, &fake(), 0),
                Pass::CaughtUp { written: 2 }
            );
            let Opened::Current(index) = vectors::open(&path, &model(), 3) else {
                panic!("the sidecar was not current");
            };
            let mut ours = Writer::resume(&path, &index).expect("resume");
            let mut theirs = Writer::resume(&path, &index).expect("resume");
            theirs.append(&second[0]).expect("their append");
            drop(theirs);
            ours.append(&row(2, "three")).expect("our append");
            drop(ours);

            match vectors::open(&path, &model(), vectors::record_length(&snapshot)) {
                Opened::Rebuild(_) => {}
                Opened::Current(index) => assert!(
                    !agrees(&index, &owed(&snapshot)),
                    "rows {:?} were read as the record's",
                    places(&index.rows)
                ),
                Opened::Missing => panic!("the sidecar went"),
            }
            assert!(!ready(&path, &snapshot, &model(), 3));
            assert_eq!(
                readiness(&path, &snapshot, &model()).state,
                Standing::Rebuilding
            );
            assert_eq!(
                catch_up(&path, &snapshot, &fake(), 0),
                Pass::CaughtUp { written: 3 }
            );
            let rows = rows_of(&path, &snapshot);
            assert_eq!(places(&rows), vec![(0, 0), (1, 0), (2, 0)]);
            assert_eq!(rows[2].digest, vectors::digest("three"));
        }
    }

    /// RETRIEVE-3: a conversation that met untrusted content writes no sidecar and asks the
    /// endpoint nothing.
    #[test]
    fn an_untrusted_conversation_writes_no_vectors_and_asks_for_none() {
        let (_directory, path) = scratch();
        let mut conversation = said(&["summarise the page"]);
        conversation.observed(Label::untrusted_public());
        let never = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: 0,
        };

        assert_eq!(
            catch_up(&path, &conversation.snapshot(), &never, 0),
            Pass::CaughtUp { written: 0 }
        );
        assert!(!path.exists());
        assert_eq!(never.calls.load(Ordering::SeqCst), 0);
    }

    /// RETRIEVE-3: a conversation that was embedded and then met untrusted content keeps none of
    /// the rows it had, and the pass that finds it so asks the endpoint nothing.
    #[test]
    fn a_conversation_that_turns_untrusted_keeps_no_rows_and_asks_for_none() {
        let (_directory, path) = scratch();
        let mut conversation = said(&["deploy on monday", "noted"]);
        let snapshot = conversation.snapshot();
        assert_eq!(
            catch_up(&path, &snapshot, &fake(), 0),
            Pass::CaughtUp { written: 2 }
        );
        assert_eq!(rows_of(&path, &snapshot).len(), 2);

        conversation.observed(Label::untrusted_public());
        let untrusted = conversation.snapshot();
        let never = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: 0,
        };
        assert_eq!(
            catch_up(&path, &untrusted, &never, vectors::generation(&path)),
            Pass::CaughtUp { written: 0 }
        );

        assert_eq!(rows_of(&path, &untrusted), Vec::new());
        assert_eq!(never.calls.load(Ordering::SeqCst), 0);
    }

    /// RETRIEVE-21: the vectors are ready for the messages before a place once every chunk of
    /// them is written, and never while one is missing or the rows disagree with the record.
    #[test]
    fn the_vectors_are_ready_once_every_chunk_before_the_place_is_written() {
        let (_directory, path) = scratch();
        let mut conversation = said(&["alpha", "beta"]);
        assert!(ready(&path, &conversation.snapshot(), &model(), 0));
        assert!(!ready(&path, &conversation.snapshot(), &model(), 1));

        catch_up(&path, &conversation.snapshot(), &fake(), 0);
        conversation.push(Message::user("gamma"));
        let snapshot = conversation.snapshot();
        assert!(ready(&path, &snapshot, &model(), 2));
        assert!(!ready(&path, &snapshot, &model(), 3));

        let mut rows = rows_of(&path, &snapshot);
        rows[0].range = 0..2;
        let mut writer = Writer::create(&path, &model()).expect("create");
        for row in &rows {
            writer.append(row).expect("append");
        }
        assert!(!ready(&path, &snapshot, &model(), 2));
    }

    /// RETRIEVE-29: how far the sidecar has caught up is counted in whole messages, a sidecar to
    /// be built again reads as rebuilding, and an untrusted conversation owes nothing.
    #[test]
    fn readiness_counts_whole_messages_and_names_a_rebuild() {
        let (_directory, path) = scratch();
        let long = "word ".repeat(vectors::CHUNK_TOKENS * 4);
        let lines: Vec<String> = (0..MAX_BATCH)
            .map(|at| format!("message number {at}"))
            .chain([long])
            .collect();
        let snapshot = said(&lines.iter().map(String::as_str).collect::<Vec<_>>()).snapshot();
        let eligible = MAX_BATCH + 1;
        assert_eq!(
            readiness(&path, &snapshot, &model()),
            Readiness {
                state: Standing::Indexing,
                indexed: 0,
                eligible
            }
        );

        let faltering = Faltering {
            calls: AtomicUsize::new(0),
            fails_from: 1,
        };
        catch_up(&path, &snapshot, &faltering, 0);
        assert_eq!(
            readiness(&path, &snapshot, &model()),
            Readiness {
                state: Standing::Indexing,
                indexed: MAX_BATCH,
                eligible
            }
        );
        catch_up(&path, &snapshot, &fake(), 0);
        assert_eq!(
            readiness(&path, &snapshot, &model()),
            Readiness {
                state: Standing::Ready,
                indexed: eligible,
                eligible
            }
        );

        let other = Model {
            id: "another".into(),
            ..model()
        };
        assert_eq!(
            readiness(&path, &snapshot, &other).state,
            Standing::Rebuilding
        );
        let mut untrusted = said(&["summarise the page"]);
        untrusted.observed(Label::untrusted_public());
        assert_eq!(
            readiness(&path, &untrusted.snapshot(), &model()).state,
            Standing::Untrusted
        );
    }

    /// Holds every call until it is let go, and says when one is being held.
    #[derive(Clone, Default)]
    struct Gate(Arc<(Mutex<(bool, bool)>, Condvar)>);

    impl Gate {
        fn open(&self) {
            self.0.0.lock().expect("lock").1 = true;
            self.0.1.notify_all();
        }
        fn hold(&self) {
            let mut state = self.0.0.lock().expect("lock");
            state.0 = true;
            self.0.1.notify_all();
            let _open = self.0.1.wait_while(state, |state| !state.1).expect("wait");
        }
        fn holding(&self) {
            let state = self.0.0.lock().expect("lock");
            let _held = self.0.1.wait_while(state, |state| !state.0).expect("wait");
        }
    }

    struct Held(Gate);

    impl Embedder for Held {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            self.0.hold();
            fake().embed(kind, texts)
        }
        fn endpoint(&self) -> String {
            fake().endpoint()
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn dimensions(&self) -> usize {
            DIMENSIONS
        }
    }

    /// RETRIEVE-4: offering a pass never waits, however slow the endpoint is. A full queue
    /// drops the pass it cannot hold, and what that pass would have embedded is embedded by a
    /// later one from the record.
    #[test]
    fn a_full_queue_drops_the_pass_and_a_later_one_embeds_what_it_would_have() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        let gate = Gate::default();
        let held = gate.clone();
        let indexer = Indexer::start(Box::new(move |_| Some(Box::new(Held(held.clone())))));
        let job = |lines: &[&str]| Job::new(directory.path(), "a1b2", said(lines).snapshot(), 1);

        assert!(indexer.offer(job(&["one"])));
        gate.holding();
        for _ in 0..QUEUE {
            assert!(indexer.offer(job(&["one"])));
        }
        assert!(!indexer.offer(job(&["one", "two"])));
        gate.open();
        indexer.settle();
        assert_eq!(
            places(&rows_of(&path, &said(&["one"]).snapshot())),
            vec![(0, 0)]
        );

        let later = said(&["one", "two", "three"]).snapshot();
        assert!(indexer.offer(Job::new(directory.path(), "a1b2", later.clone(), 2)));
        indexer.settle();
        assert_eq!(
            places(&rows_of(&path, &later)),
            vec![(0, 0), (1, 0), (2, 0)]
        );
    }

    /// Writes one decision to the trail it is given for each call, as a client's gates do.
    struct Audited(SharedSink);

    impl Embedder for Audited {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            self.0.lock().expect("lock").emit(Event::GatePassed {
                gate: "egress",
                detail: "embeddings".to_string(),
            });
            fake().embed(kind, texts)
        }
        fn endpoint(&self) -> String {
            fake().endpoint()
        }
        fn model(&self) -> &str {
            "fake"
        }
        fn dimensions(&self) -> usize {
            DIMENSIONS
        }
    }

    /// RETRIEVE-22: what the gates decided about a pass's requests is in the session's own trail,
    /// under the turn whose save offered the pass.
    #[test]
    fn a_pass_writes_what_its_requests_gates_decided_to_the_sessions_trail() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let embedders: Embedders = Box::new(|sink| Some(Box::new(Audited(sink))));
        let job = Job::new(directory.path(), "a1b2", said(&["hello"]).snapshot(), 7);

        assert_eq!(
            run(&embedders, &|| true, &job),
            Pass::CaughtUp { written: 1 }
        );

        let trail =
            std::fs::read_to_string(directory.path().join("a1b2.audit.jsonl")).expect("a trail");
        let lines: Vec<serde_json::Value> = trail
            .lines()
            .map(|line| serde_json::from_str(line).expect("json"))
            .collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["turn"], 7);
        assert!(lines[0]["event"].to_string().contains("egress"));
    }

    /// Embeds as the fake does, under the endpoint, model and length it is given.
    #[derive(Clone, Copy, Debug)]
    struct Served {
        endpoint: &'static str,
        model: &'static str,
        dimensions: usize,
    }

    impl Embedder for Served {
        fn embed(&self, kind: Kind, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            FakeEmbedder::new(self.dimensions).embed(kind, texts)
        }
        fn endpoint(&self) -> String {
            self.endpoint.to_string()
        }
        fn model(&self) -> &str {
            self.model
        }
        fn dimensions(&self) -> usize {
            self.dimensions
        }
    }

    const SERVED: Served = Served {
        endpoint: "https://a.example/v1/embeddings",
        model: "embed-a",
        dimensions: DIMENSIONS,
    };

    fn current_under(path: &Path, snapshot: &Snapshot, served: &Served) -> Vec<Row> {
        let model = model_of(served).expect("a model");
        match vectors::open(path, &model, vectors::record_length(snapshot)) {
            Opened::Current(index) => index.rows,
            other => panic!("the sidecar was not current under {served:?}: {other:?}"),
        }
    }

    /// RETRIEVE-5: a change of endpoint, of model or of length rebuilds the whole sidecar on the
    /// next pass, never appending to rows from the one before, and switching back rebuilds again.
    #[test]
    fn switching_the_endpoint_model_or_length_rebuilds_without_mixing() {
        let switches = [
            Served {
                endpoint: "https://b.example/v1/embeddings",
                ..SERVED
            },
            Served {
                model: "embed-b",
                ..SERVED
            },
            Served {
                dimensions: DIMENSIONS * 2,
                ..SERVED
            },
        ];
        for other in switches {
            let (_directory, path) = scratch();
            let mut conversation = said(&["the release train", "it leaves on thursday"]);
            assert_eq!(
                catch_up(&path, &conversation.snapshot(), &SERVED, 0),
                Pass::CaughtUp { written: 2 }
            );
            conversation.push(Message::user("and the hotfix?"));
            let snapshot = conversation.snapshot();
            let other_model = model_of(&other).expect("a model");
            assert_eq!(
                readiness(&path, &snapshot, &other_model).state,
                Standing::Rebuilding
            );
            assert!(!ready(&path, &snapshot, &other_model, 1));

            assert_eq!(
                catch_up(&path, &snapshot, &other, 0),
                Pass::CaughtUp { written: 3 },
                "the rows from {SERVED:?} were kept for {other:?}"
            );
            let rows = current_under(&path, &snapshot, &other);
            assert_eq!(places(&rows), vec![(0, 0), (1, 0), (2, 0)]);
            assert!(rows.iter().all(|row| row.vector.len() == other.dimensions));
            let served = model_of(&SERVED).expect("a model");
            assert!(matches!(
                vectors::open(&path, &served, 3),
                Opened::Rebuild(_)
            ));

            assert_eq!(
                catch_up(&path, &snapshot, &SERVED, 0),
                Pass::CaughtUp { written: 3 }
            );
            assert_eq!(
                catch_up(&path, &snapshot, &SERVED, 0),
                Pass::CaughtUp { written: 0 }
            );
            assert_eq!(current_under(&path, &snapshot, &SERVED).len(), 3);
        }
    }

    /// RETRIEVE-5: the indexer asks which endpoint to embed with as each pass starts, so a choice
    /// changed between passes rebuilds the sidecar for the new one, and back again.
    #[test]
    fn the_indexer_rebuilds_for_the_endpoint_chosen_as_a_pass_starts() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        let chosen = Arc::new(Mutex::new(SERVED));
        let choice = chosen.clone();
        let indexer = Indexer::start(Box::new(move |_| {
            Some(Box::new(*choice.lock().expect("lock")) as Box<dyn Embedder>)
        }));
        let snapshot = said(&["one", "two"]).snapshot();
        let pass = || {
            assert!(indexer.offer(Job::new(directory.path(), "a1b2", snapshot.clone(), 1)));
            indexer.settle();
        };
        let other = Served {
            endpoint: "https://b.example/v1/embeddings",
            ..SERVED
        };

        pass();
        assert_eq!(current_under(&path, &snapshot, &SERVED).len(), 2);
        *chosen.lock().expect("lock") = other;
        pass();
        assert_eq!(current_under(&path, &snapshot, &other).len(), 2);
        *chosen.lock().expect("lock") = SERVED;
        pass();
        assert_eq!(current_under(&path, &snapshot, &SERVED).len(), 2);
    }

    /// RETRIEVE-29: a pass the endpoint gave no vectors names why until a pass makes progress, and
    /// only for that endpoint.
    #[test]
    fn the_indexer_keeps_why_the_endpoint_gave_nothing_until_a_pass_gets_on() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        let failing = Arc::new(Mutex::new(Some(401)));
        let status = failing.clone();
        let indexer = Indexer::start(Box::new(move |_| {
            let failing = *status.lock().expect("lock");
            Some(Box::new(FakeEmbedder { failing, ..fake() }) as Box<dyn Embedder>)
        }));
        let snapshot = said(&["one", "two"]).snapshot();
        let pass = || {
            assert!(indexer.offer(Job::new(directory.path(), "a1b2", snapshot.clone(), 1)));
            indexer.settle();
        };
        let endpoint = fake().endpoint();

        pass();
        assert_eq!(indexer.failure(&path, &endpoint), Some(Failure::Key));
        assert_eq!(
            indexer.failure(&path, "https://b.example/v1/embeddings"),
            None
        );
        *failing.lock().expect("lock") = Some(503);
        pass();
        assert_eq!(indexer.failure(&path, &endpoint), Some(Failure::Failed));
        *failing.lock().expect("lock") = None;
        pass();
        assert_eq!(indexer.failure(&path, &endpoint), None);
    }

    /// RETRIEVE-33: a pass the settings give no endpoint for leaves the vectors as they are, rather
    /// than rebuilding them for no model.
    #[test]
    fn a_pass_with_no_endpoint_keeps_the_vectors_it_finds() {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = vectors::path_of(directory.path(), "a1b2");
        let first = said(&["one"]).snapshot();
        let embedding: Embedders = Box::new(|_| Some(Box::new(fake())));
        let job = |snapshot: Snapshot| Job::new(directory.path(), "a1b2", snapshot, 1);
        assert_eq!(
            run(&embedding, &|| true, &job(first.clone())),
            Pass::CaughtUp { written: 1 }
        );
        let before = std::fs::read(&path).expect("vectors");

        let unconfigured: Embedders = Box::new(|_| None);
        assert_eq!(
            run(
                &unconfigured,
                &|| true,
                &job(said(&["one", "two"]).snapshot())
            ),
            Pass::Unconfigured
        );
        assert_eq!(std::fs::read(&path).expect("vectors"), before);
    }
}
