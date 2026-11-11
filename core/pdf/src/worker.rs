//! one background thread that owns the pdf engine and the tile cache.
//!
//! every window talks to the same worker through a cheap, cloneable
//! handle. that is how many windows share one engine and one cache
//! instead of each paying for their own.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use crate::cache::{TileBytes, TileCache};
use crate::engine::PdfEngine;
use crate::geometry::{PageSize, TileKey};
use crate::outline::OutlineItem;
use crate::queue::NewestFirstQueue;
use crate::search::{SearchMatch, SearchQuery, SearchResults, MAX_SEARCH_MATCHES};
use crate::{DocumentId, PdfError};

/// what the viewer needs to know about a freshly opened document.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentInfo {
    pub id: DocumentId,
    pub page_sizes: Vec<PageSize>,
}

/// a snapshot of how much the worker is holding, for the status bar and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerStats {
    pub open_documents: usize,
    pub cached_tiles: usize,
    pub cached_bytes: usize,
}

type Reply<T> = Sender<Result<T, PdfError>>;

/// pdfium keeps global state, so only one thread in the whole process may
/// be inside it at a time. the app starts a single worker, but this lock
/// keeps things correct even if a second one is ever started (the tests do).
static PDFIUM_LOCK: Mutex<()> = Mutex::new(());

fn lock_pdfium() -> MutexGuard<'static, ()> {
    // a panic while drawing must not lock every other document out for good
    PDFIUM_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

enum Request {
    Open {
        path: PathBuf,
        password: Option<String>,
        reply: Reply<DocumentInfo>,
    },
    Close {
        id: DocumentId,
    },
    Tile {
        key: TileKey,
        reply: Reply<TileBytes>,
    },
    Stats {
        reply: Reply<WorkerStats>,
    },
    Outline {
        id: DocumentId,
        reply: Reply<Vec<OutlineItem>>,
    },
    /// searches one slice of pages (see `RenderWorker::search`)
    SearchSlice {
        id: DocumentId,
        query: SearchQuery,
        first_page: u32,
        limit: usize,
        reply: Reply<SearchSlice>,
    },
}

/// what one slice of a search found, and where the next slice starts.
struct SearchSlice {
    matches: Vec<SearchMatch>,
    /// none when the slice reached the last page
    next_page: Option<u32>,
}

/// how many pages one search request reads before letting tiles through.
///
/// the worker does one thing at a time, so a search of a whole 500 page
/// document in one go would hold up every tile for as long as it takes
/// (seconds on a long file), and scrolling would freeze. searching 20
/// pages at a time takes a few tens of milliseconds, and between slices
/// the worker draws waiting tiles, so the page keeps filling in while
/// the search runs.
const PAGES_PER_SEARCH_SLICE: u32 = 20;

/// the handle the rest of the app uses. cloning it is cheap, and every
/// clone talks to the same thread.
#[derive(Clone)]
pub struct RenderWorker {
    requests: Sender<Request>,
}

impl RenderWorker {
    /// starts the worker thread and loads pdfium from `library_dir`.
    ///
    /// `cache_budget_bytes` is the most memory the tile cache may use,
    /// shared by all documents and windows.
    pub fn start(library_dir: &Path, cache_budget_bytes: usize) -> Result<Self, PdfError> {
        let (requests, inbox) = mpsc::channel();
        let (ready, ready_inbox) = mpsc::channel();
        let library_dir = library_dir.to_path_buf();

        thread::Builder::new()
            .name("pdf-worker".into())
            .spawn(move || match load_engine(&library_dir) {
                Ok(engine) => {
                    let _ = ready.send(Ok(()));
                    serve_requests(engine, TileCache::new(cache_budget_bytes), inbox);
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|error| PdfError::EngineUnavailable(error.to_string()))?;

        // wait until the library has loaded so a missing library is
        // reported right here instead of on the first open
        ready_inbox.recv().map_err(|_| PdfError::WorkerStopped)??;
        Ok(Self { requests })
    }

    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<DocumentInfo, PdfError> {
        self.ask(|reply| Request::Open {
            path: path.to_path_buf(),
            password: password.map(str::to_owned),
            reply,
        })
    }

    /// closes a document and frees its cached tiles. safe to call twice.
    pub fn close(&self, id: DocumentId) {
        let _ = self.requests.send(Request::Close { id });
    }

    /// returns one tile as png bytes, from the cache when possible.
    pub fn tile(&self, key: TileKey) -> Result<TileBytes, PdfError> {
        self.ask(|reply| Request::Tile { key, reply })
    }

    pub fn stats(&self) -> Result<WorkerStats, PdfError> {
        self.ask(|reply| Request::Stats { reply })
    }

    /// the document's table of contents, flattened in reading order.
    /// empty when the pdf has none.
    pub fn outline(&self, id: DocumentId) -> Result<Vec<OutlineItem>, PdfError> {
        self.ask(|reply| Request::Outline { id, reply })
    }

    /// finds every place `text` appears, ignoring case, page by page.
    ///
    /// the search is sent to the worker in slices of pages, waiting for
    /// each answer before sending the next, so tile requests from the
    /// windows are served in between (see `PAGES_PER_SEARCH_SLICE`).
    pub fn search(&self, id: DocumentId, text: &str) -> Result<SearchResults, PdfError> {
        let Some(query) = SearchQuery::new(text) else {
            return Ok(SearchResults::default());
        };
        let mut matches = Vec::new();
        let mut next_page = Some(0);
        while let Some(first_page) = next_page {
            // one match more than the limit is gathered, so "there were
            // more" can be told apart from "there were exactly 500"
            if matches.len() > MAX_SEARCH_MATCHES {
                break;
            }
            let limit = MAX_SEARCH_MATCHES + 1 - matches.len();
            let slice = self.ask(|reply| Request::SearchSlice {
                id,
                query: query.clone(),
                first_page,
                limit,
                reply,
            })?;
            matches.extend(slice.matches);
            next_page = slice.next_page;
        }
        let truncated = matches.len() > MAX_SEARCH_MATCHES;
        matches.truncate(MAX_SEARCH_MATCHES);
        Ok(SearchResults { matches, truncated })
    }

    /// sends a request and waits for its answer.
    fn ask<T>(&self, build: impl FnOnce(Reply<T>) -> Request) -> Result<T, PdfError> {
        let (reply, answer) = mpsc::channel();
        self.requests
            .send(build(reply))
            .map_err(|_| PdfError::WorkerStopped)?;
        answer.recv().map_err(|_| PdfError::WorkerStopped)?
    }
}

fn load_engine(library_dir: &Path) -> Result<PdfEngine, PdfError> {
    let _pdfium = lock_pdfium();
    PdfEngine::load(library_dir)
}

/// how many tile requests may wait at once. a screen needs a few dozen
/// at most, so anything beyond this is left over from fast scrolling.
const MAX_WAITING_TILES: usize = 96;

struct WaitingTile {
    key: TileKey,
    reply: Reply<TileBytes>,
}

/// the worker thread's loop. it ends when every handle has been dropped.
///
/// opening, closing, stats, the outline and one slice of a search are
/// quick and handled as they arrive.
/// tiles are slow, so they wait in a newest-first queue (see queue.rs)
/// and are drawn one at a time between checks of the inbox.
fn serve_requests(mut engine: PdfEngine, mut cache: TileCache, inbox: Receiver<Request>) {
    let mut waiting_tiles = NewestFirstQueue::new(MAX_WAITING_TILES);
    loop {
        if waiting_tiles.is_empty() {
            // nothing to draw: sleep until a request arrives
            match inbox.recv() {
                Ok(request) => accept(request, &mut engine, &mut cache, &mut waiting_tiles),
                Err(_) => return,
            }
        }
        while let Ok(request) = inbox.try_recv() {
            accept(request, &mut engine, &mut cache, &mut waiting_tiles);
        }
        if let Some(tile) = waiting_tiles.pop_newest() {
            let _pdfium = lock_pdfium();
            let _ = tile
                .reply
                .send(cached_or_rendered(&engine, &mut cache, tile.key));
        }
    }
}

/// handles a quick request right away, or puts a tile request in the queue.
fn accept(
    request: Request,
    engine: &mut PdfEngine,
    cache: &mut TileCache,
    waiting_tiles: &mut NewestFirstQueue<WaitingTile>,
) {
    let _pdfium = lock_pdfium();
    match request {
        Request::Open {
            path,
            password,
            reply,
        } => {
            let result = engine
                .open(&path, password.as_deref())
                .map(|(id, page_sizes)| DocumentInfo { id, page_sizes });
            let _ = reply.send(result);
        }
        Request::Close { id } => {
            engine.close(id);
            cache.remove_where(|key| key.document == id);
        }
        Request::Tile { key, reply } => {
            if let Some(given_up) = waiting_tiles.push(WaitingTile { key, reply }) {
                let _ = given_up.reply.send(Err(PdfError::Superseded));
            }
        }
        Request::Stats { reply } => {
            let _ = reply.send(Ok(WorkerStats {
                open_documents: engine.open_document_count(),
                cached_tiles: cache.len(),
                cached_bytes: cache.used_bytes(),
            }));
        }
        Request::Outline { id, reply } => {
            let _ = reply.send(engine.outline(id));
        }
        Request::SearchSlice {
            id,
            query,
            first_page,
            limit,
            reply,
        } => {
            let _ = reply.send(search_slice(engine, id, &query, first_page, limit));
        }
    }
}

fn search_slice(
    engine: &PdfEngine,
    id: DocumentId,
    query: &SearchQuery,
    first_page: u32,
    limit: usize,
) -> Result<SearchSlice, PdfError> {
    let page_count = engine.page_count(id)?;
    let end = first_page
        .saturating_add(PAGES_PER_SEARCH_SLICE)
        .min(page_count);
    let matches = engine.search_pages(id, query, first_page..end, limit)?;
    let next_page = (end < page_count).then_some(end);
    Ok(SearchSlice { matches, next_page })
}

fn cached_or_rendered(
    engine: &PdfEngine,
    cache: &mut TileCache,
    key: TileKey,
) -> Result<TileBytes, PdfError> {
    if let Some(tile) = cache.get(&key) {
        return Ok(tile);
    }
    let tile = Arc::new(engine.render_tile(key)?);
    cache.insert(key, tile.clone());
    Ok(tile)
}
