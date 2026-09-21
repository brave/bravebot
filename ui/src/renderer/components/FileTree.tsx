import { FilePreview } from './FilePreview'
import type { FileSearch } from '../../shared/files'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Fold } from './Fold'
import { FileGlyph } from './FileGlyph'
import { type FileRow, type Listing, isSubpath, under } from '../../shared/files'

/**
 * The folder the session is working in.
 *
 * The other panels in this column are derived from the transcript — what the session *touched*.
 * This one is the exception, and reads the disk: what is *there*. A double-click hands a file to
 * whichever app the system assigns its type, which is the one thing this window could not do
 * about a filename it had been showing all along.
 *
 * Listed a directory at a time, when somebody expands one, rather than walked up front. A project
 * with a `node_modules` in it would otherwise cost tens of thousands of syscalls to draw a panel
 * nobody had asked to open yet — and the listing that matters is the one under the cursor.
 *
 * Every path here is relative to the session's own directory and `''` is that directory: the
 * renderer never learns an absolute path and never composes one. `shared/files.ts` says why, and
 * `main/files.ts` is the half of the promise that holds even when a symlink points out of the
 * project.
 */
export function FileTree({
  session,
  root,
  running,
}: {
  /** The session's handle. What a relative path is relative to, as far as the main process. */
  session: string
  /** The directory itself, for the one line that says which folder this is. */
  root: string
  /** Whether a turn is in flight. The falling edge is when the tree is worth re-reading. */
  running: boolean
}): React.JSX.Element {
  const [listings, setListings] = useState<ReadonlyMap<string, Listing>>(new Map())
  /** The directories that are open. Held here rather than per row so a refresh can re-read them. */
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set())
  /** Directories asked about that answered nothing — permissions, or gone since. */
  const [unreadable, setUnreadable] = useState<ReadonlySet<string>>(new Set())
  const [hidden, setHidden] = useState(false)
  const [query, setQuery] = useState('')
  const [searchOpen, setSearchOpen] = useState(false)
  const searchButton = useRef<HTMLButtonElement>(null)
  const closeSearch = () => { setQuery(''); setSearchOpen(false); searchButton.current?.focus() }
  const [results, setResults] = useState<FileSearch | null>(null)
  const [searching, setSearching] = useState(false)
  const [previewPath, setPreviewPath] = useState<string | null>(null)
  useEffect(() => {
    if (!query.trim()) { setResults(null); setSearching(false); return }
    let gone = false
    setResults(null)
    setSearching(true)
    const timer = setTimeout(() => { void window.bravebot.searchFiles(session, query, hidden).then((value) => {
      if (!gone) setResults(value)
    }).catch(() => { if (!gone) setProblem('Project search failed. Try again.') }).finally(() => { if (!gone) setSearching(false) }) }, 180)
    return () => { gone = true; clearTimeout(timer) }
  }, [session, query, hidden])
  const [problem, setProblem] = useState<string | null>(null)

  /**
   * Read one directory.
   *
   * A listing that comes back `null` is recorded as such rather than left absent: absent means
   * "not asked yet", and a row that says nothing under a directory somebody just expanded is the
   * panel implying the folder is empty when it could not read it.
   */
  const load = useCallback(
    async (path: string) => {
      const listing = await window.bravebot.listFiles(session, path)
      if (listing === null) {
        setUnreadable((old) => new Set(old).add(path))
        setListings((old) => {
          const next = new Map(old)
          next.delete(path)
          return next
        })
        return
      }
      setUnreadable((old) => {
        if (!old.has(path)) return old
        const next = new Set(old)
        next.delete(path)
        return next
      })
      setListings((old) => new Map(old).set(path, listing))
    },
    [session],
  )

  // The root, once. Everything below it waits to be asked for.
  useEffect(() => {
    void load('')
  }, [load])

  /** Read the root and every open directory again. */
  const refresh = useCallback(async () => {
    setProblem(null)
    await Promise.all([load(''), ...[...open].map((path) => load(path))])
  }, [load, open])

  // A turn that has just finished is the moment the folder can have changed, so every directory
  // currently on screen is read again. Cheaper than watching the tree, and quiet when nothing is
  // running: the alternative is a watcher per session firing on every write inside `target/`.
  //
  // The falling edge, not `running` itself — a re-read on the way *into* a turn would be a listing
  // of the folder as it was before the turn touched anything.
  const wasRunning = useRef(running)
  useEffect(() => {
    const finished = wasRunning.current && !running
    wasRunning.current = running
    if (finished) void refresh()
  }, [running, refresh])

  const toggle = useCallback(
    (path: string) => {
      setOpen((old) => {
        const next = new Set(old)
        if (next.has(path)) next.delete(path)
        else next.add(path)
        return next
      })
      // Read on the way open, and only the first time: a directory already listed opens instantly,
      // and the turn-end refresh above is what keeps it honest afterwards.
      if (!open.has(path) && !listings.has(path)) void load(path)
    },
    [listings, load, open],
  )

  const show = useCallback(
    async (path: string) => {
      // Checked here as well as in the main process. This one cannot be the whole of the promise —
      // it runs in the process that would be doing the asking — but it means a bug in this file
      // fails as a message rather than as a request.
      if (!isSubpath(path) || path === '') return
      setPreviewPath(path)
    },
    [session],
  )

  const rootListing = listings.get('')
  // Every whitespace-separated term has to appear, in any order, as a plain substring — the same
  // bargain the session filter strikes, and for the same reason: a fuzzy score on a folder listing
  // mostly buys the right to return rows the reader cannot account for.
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean)

  return (
    <div className="tree">
      <div className="tree-tools">
        {/* The whole path, because the panel head says only "Files" and two sessions in sibling
            checkouts are otherwise indistinguishable here. Ellipsised, with the tooltip carrying
            it back — the same bargain the file lists above strike. */}
        <code className="tree-root" title={root}>
          {root}
        </code>
        <button ref={searchButton} className={`tree-tool ${searchOpen ? 'on' : ''}`}
          title="Search files" aria-label="Search files" aria-expanded={searchOpen}
          onClick={() => searchOpen ? closeSearch() : setSearchOpen(true)}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
        </button>
        {/* Labelled with the thing it is about rather than with an eye or a dot: `.*` is what a
            dotfile looks like, and it is legible at 10px where a pictogram is not. */}
        <button
          className={`tree-tool dotfiles ${hidden ? 'on' : ''}`}
          aria-pressed={hidden}
          title={hidden ? 'Hide dotfiles' : 'Show dotfiles'}
          onClick={() => setHidden(!hidden)}
        >
          .*
        </button>
        <button className="tree-tool" title="Read the folder again" onClick={() => void refresh()}>
          ↻
        </button>
      </div>

      {searchOpen && <div className="tree-search">
        <input autoFocus type="search" className="tree-find" value={query}
          placeholder="Search project filenames…" aria-label="Search project files by name"
          onChange={event => setQuery(event.target.value)}
          onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); closeSearch() } }} />
        <button className="tree-tool" aria-label="Close file search" onClick={closeSearch}>×</button>
      </div>}

      {problem && <p className="tree-problem">{problem}</p>}

      {/* The rows sit in a well of their own rather than straight on the column. Everything else
          in this panel is a short list of names the session mentioned; this is a folder somebody
          scans, and on the sidebar's own ground it read as text floating in the column with no
          edge to say where it began. The well also gives the tree somewhere to scroll: a project
          with forty things in its root would otherwise push the column's own scrollbar down and
          take the header with it. */}
      <div className="tree-body">
        {terms.length > 0 ? <div className="file-search-results">
          {searching && <p role="status">Searching project…</p>}
          {!searching && results?.paths.length === 0 && <p>No matching files.</p>}
          {results?.paths.map((path) => <button key={path} onClick={() => setPreviewPath(path)} title={path}>{path}</button>)}
          <p className="tree-note">Search skips .git, node_modules, target and dist. Symbolic-link directories are not followed.</p>
          {results?.incomplete && <p role="status">Results are limited or some folders could not be read. Narrow the search.</p>}
        </div> : rootListing === undefined ? (
          <p className="none">{unreadable.has('') ? 'That folder cannot be read.' : 'Reading…'}</p>
        ) : (
          <Rows
            path=""
            listing={rootListing}
            listings={listings}
            open={open}
            unreadable={unreadable}
            hidden={hidden}
            depth={0}
            terms={terms}
            onToggle={toggle}
            onOpen={show}
            tree
          />
        )}
      </div>
      {previewPath && <FilePreview session={session} path={previewPath} onClose={() => setPreviewPath(null)} />}
    </div>
  )
}

/** How many rows one directory has, once the toggle has had its say. */
function visible(rows: readonly FileRow[], hidden: boolean): FileRow[] {
  return hidden ? [...rows] : rows.filter((row) => !row.hidden)
}

/** Whether a name answers every term. */
function answers(name: string, terms: readonly string[]): boolean {
  const haystack = name.toLowerCase()
  return terms.every((term) => haystack.includes(term))
}

/**
 * Whether anything under this directory answers, in what has been read.
 *
 * A folder is kept when its own name matches *or* something inside it does — otherwise a query
 * would delete the path to its own results. Bounded by what is loaded rather than by what is on
 * disk: this cannot go and read a folder nobody opened, which is exactly why the panel says so
 * out loud while a query is running.
 */
function holds(
  path: string,
  listings: ReadonlyMap<string, Listing>,
  terms: readonly string[],
  hidden: boolean,
): boolean {
  const listing = listings.get(path)
  if (listing === undefined) return false
  return visible(listing.rows, hidden).some(
    (row) =>
      answers(row.name, terms) ||
      (row.kind === 'directory' && holds(under(path, row.name), listings, terms, hidden)),
  )
}

/**
 * One directory's rows, and recursively the open ones below them.
 *
 * `role="tree"` at the top and `role="group"` under each open directory, so this reads out as one
 * tree rather than as a stack of unrelated lists. The children of an open directory sit inside a
 * `Fold`, which is what the panels, the session groups and the transcript's runs of tool calls all
 * use — a fourth kind of fold in this window that moved differently would read as a different
 * idea.
 */
function Rows({
  path,
  listing,
  listings,
  open,
  unreadable,
  hidden,
  depth,
  terms,
  onToggle,
  onOpen,
  tree = false,
}: {
  path: string
  listing: Listing
  listings: ReadonlyMap<string, Listing>
  open: ReadonlySet<string>
  unreadable: ReadonlySet<string>
  hidden: boolean
  depth: number
  /** The query, split. Empty when nothing is being filtered. */
  terms: readonly string[]
  onToggle: (path: string) => void
  onOpen: (path: string) => void
  /** Whether this is the outermost list, which is the tree rather than a group inside one. */
  tree?: boolean
}): React.JSX.Element {
  const all = visible(listing.rows, hidden)
  const rows =
    terms.length === 0
      ? all
      : all.filter(
          (row) =>
            answers(row.name, terms) ||
            (row.kind === 'directory' &&
              holds(under(path, row.name), listings, terms, hidden)),
        )

  if (rows.length === 0) {
    return (
      <p className="none">
        {terms.length > 0
          ? 'Nothing read so far matches.'
          : listing.rows.length === 0
            ? 'This folder is empty.'
            : 'Everything here is hidden.'}
      </p>
    )
  }

  return (
    <ul className="tree-list" role={tree ? 'tree' : 'group'}>
      {rows.map((row) => {
        const here = under(path, row.name)
        const below = listings.get(here)
        // A query opens whatever it found things in, for as long as it runs. A folder shown
        // *because* of its contents and still shut would be the panel answering a question with
        // the answer hidden. Read through `open` rather than written into it, as the session
        // list's groups are, so nobody's folds are rearranged by typing.
        const expanded =
          open.has(here) ||
          (terms.length > 0 &&
            !answers(row.name, terms) &&
            holds(here, listings, terms, hidden))
        return (
          <li
            key={row.name}
            role="treeitem"
            aria-expanded={row.kind === 'directory' ? expanded : undefined}
            aria-level={depth + 1}
          >
            {/* The depth rides on a custom property rather than nested padding, so a row six
                folders deep still ellipsises against the column's own edge instead of against a
                box that has been indented out of it. */}
            <button
              className={`tree-row ${row.kind}`}
              style={{ '--depth': depth } as React.CSSProperties}
              title={row.kind === 'directory' ? row.name : `Open ${row.name}`}
              // A double-click is how a file is opened, which is what a file list has meant since
              // before this app existed. Enter does the same thing for anybody who reached the row
              // by tab — a control that needs a mouse is a control half the users do not have.
              onClick={row.kind === 'directory' ? () => onToggle(here) : undefined}
              onDoubleClick={row.kind === 'file' ? () => onOpen(here) : undefined}
              onKeyDown={
                row.kind === 'file'
                  ? (event) => {
                      if (event.key !== 'Enter') return
                      event.preventDefault()
                      onOpen(here)
                    }
                  : undefined
              }
            >
              <span className={`chevron ${expanded ? 'open' : ''}`} aria-hidden="true">
                {row.kind === 'directory' ? '›' : ''}
              </span>
              {/* A folder's badge is a slash, which is what a folder is called in a path. It
                  earns its place by holding the column the file badges stand in: without it the
                  names either side of a folder would not line up. */}
              {row.kind === 'directory' ? (
                <span className="tree-glyph folder" aria-hidden="true">
                  /
                </span>
              ) : (
                <FileGlyph name={row.name} />
              )}
              <span className="tree-name">{row.name}</span>
            </button>
            {row.kind === 'directory' && (
              <Fold open={expanded}>
                {unreadable.has(here) ? (
                  <p className="none">That folder cannot be read.</p>
                ) : below === undefined ? (
                  <p className="none">Reading…</p>
                ) : (
                  <Rows
                    path={here}
                    listing={below}
                    listings={listings}
                    open={open}
                    unreadable={unreadable}
                    hidden={hidden}
                    depth={depth + 1}
                    terms={terms}
                    onToggle={onToggle}
                    onOpen={onOpen}
                  />
                )}
              </Fold>
            )}
          </li>
        )
      })}
      {listing.truncated && (
        <li className="tree-more">
          Too many entries to list. What is here is the first part of the folder.
        </li>
      )}
    </ul>
  )
}
