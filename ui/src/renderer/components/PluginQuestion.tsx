import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type {
  Delta, DefinitionOffer, FileChange, HookPart, HookRow, HostListing, InstallListing, PartDeltas, PassedOver,
  PluginQuestion as Question, Reach, ServerPart, ServerRow, SkillOffer, UpdateListing,
} from '../../shared/plugins'
import { Button, Icon, Label } from '../nala'
import { Modal } from './Modal'

/**
 * The questions a `plugins.add` or `plugins.update` job puts (docs/specs/plugins.md PLUGIN-3,
 * PLUGIN-4, PLUGIN-9): the host of a git source, drawn as the fetch confirm is, and the install and
 * update sheets.
 *
 * Every string under an `untrusted` key is the plugin's own text and is drawn inside the
 * `confirm untrusted` margin, as plain text. A long description is cut, with the rest shown on
 * request and never dropped; an argv word is never cut. The yes is offered only once every row has
 * been on screen, and a cut row counts only once it is shown whole.
 *
 * `onAnswer(true)` is called only from the yes button. Esc, the close button and the no button
 * each call `onAnswer(false)`.
 */
export function PluginQuestion({ question, onAnswer }: {
  question: Question
  onAnswer: (yes: boolean) => void
}): React.JSX.Element {
  switch (question.kind) {
    case 'host': return <HostSheet listing={question.listing} onAnswer={onAnswer} />
    case 'install': return <InstallSheet listing={question.listing} onAnswer={onAnswer} />
    case 'update': return <UpdateSheet listing={question.listing} onAnswer={onAnswer} />
  }
}

/** A description longer than this is cut, with the rest behind "Show more". */
const CUT_AT = 240

// ---- reading every row --------------------------------------------------------------------------

interface Gate {
  /** Count `id` as a row to read. Returns the call that stops counting it. */
  register(id: string): () => void
  /** Count row `id` as cut, waiting on "Show more". Returns the call that stops counting it. */
  cut(id: string): () => void
  /** The element whose being on screen means row `id` has been read to its end. */
  end(id: string): (element: HTMLElement | null) => void
}
const GateContext = createContext<Gate | null>(null)

/** What is left to read: rows not yet on screen, and cut descriptions not yet shown whole. */
interface Reading {
  total: number
  read: number
  /** Unread rows that are whole, so scrolling brings them on screen. */
  below: number
  /** Descriptions still cut. */
  cut: number
}

/**
 * Which rows have been read. A row is read once the marker after its last word has been inside the
 * viewport, whatever scrolls it there; the marker of a cut row is drawn only after "Show more".
 */
function useReadingGate(): { gate: Gate } & Reading {
  const [rows, setRows] = useState<ReadonlySet<string>>(new Set())
  const [cuts, setCuts] = useState<ReadonlySet<string>>(new Set())
  const [seen, setSeen] = useState<ReadonlySet<string>>(new Set())
  const ids = useRef(new Map<Element, string>())
  const observer = useRef<IntersectionObserver | null>(null)

  useEffect(() => {
    const watch = new IntersectionObserver((entries) => {
      const now = entries.filter((entry) => entry.isIntersecting).map((entry) => ids.current.get(entry.target)).filter((id): id is string => !!id)
      if (now.length) setSeen((old) => (now.every((id) => old.has(id)) ? old : new Set([...old, ...now])))
    })
    observer.current = watch
    for (const element of ids.current.keys()) watch.observe(element)
    return () => { watch.disconnect(); observer.current = null }
  }, [])

  const gate = useMemo<Gate>(() => ({
    register(id) {
      setRows((old) => new Set([...old, id]))
      return () => setRows((old) => { const next = new Set(old); next.delete(id); return next })
    },
    cut(id) {
      setCuts((old) => new Set([...old, id]))
      return () => setCuts((old) => { const next = new Set(old); next.delete(id); return next })
    },
    end(id) {
      return (element) => {
        for (const [known, of] of ids.current) {
          if (of === id) { observer.current?.unobserve(known); ids.current.delete(known) }
        }
        if (!element) return
        ids.current.set(element, id)
        observer.current?.observe(element)
      }
    },
  }), [])
  const unread = [...rows].filter((id) => !seen.has(id))
  const cut = unread.filter((id) => cuts.has(id)).length
  return { gate, total: rows.size, read: rows.size - unread.length, below: unread.length - cut, cut }
}

/** The footer's hint while a yes is held back: the count, then what the person still has to do. */
function readingHint({ total, read, below, cut }: Reading): string {
  const counted = `${read} of ${total} rows read.`
  const cutShort = cut === 1 ? '1 description is cut short.' : `${cut} descriptions are cut short.`
  if (cut > 0 && below > 0) return `${counted} ${cutShort} Choose Show more on ${cut === 1 ? 'it' : 'each'}, and scroll to the end.`
  if (cut > 0) return `${counted} ${cutShort} Choose Show more to read ${cut === 1 ? 'it' : 'them'}.`
  return `${counted} Scroll to the end to answer yes.`
}

/**
 * One row of a sheet. Its end marker is drawn after its content unless the row is still cut. A row
 * with a `label` is a pair: the label in a fixed column and the content wrapping in the column
 * beside it.
 */
function Row({ id, cut = false, label, className = '', children }: {
  id: string
  cut?: boolean
  label?: string
  className?: string
  children: React.ReactNode
}): React.JSX.Element {
  const gate = useContext(GateContext)
  useLayoutEffect(() => gate?.register(id), [gate, id])
  useLayoutEffect(() => (cut ? gate?.cut(id) : undefined), [gate, id, cut])
  const end = useMemo(() => gate?.end(id), [gate, id])
  return (
    <li className={`plugin-sheet-row ${label !== undefined ? 'plugin-sheet-pair' : ''} ${className}`.replace(/\s+/g, ' ').trim()} data-test="plugin-sheet-row">
      {label !== undefined ? <>
        <span className="plugin-sheet-label">{label}</span>
        <span className="plugin-sheet-value">{children}</span>
      </> : children}
      {!cut && <span className="plugin-sheet-row-end" ref={end} aria-hidden="true" />}
    </li>
  )
}

/** The first `CUT_AT` characters of a long description, ended at a word where one is near. */
const cutShort = (text: string): string => {
  const head = text.slice(0, CUT_AT)
  const space = head.search(/\s\S*$/)
  return `${(space > CUT_AT * 0.75 ? head.slice(0, space) : head).trimEnd()}…`
}

/** A description, cut near `CUT_AT` with the rest shown on request. */
function Description({ id, text }: { id: string; text: string }): React.JSX.Element {
  const [whole, setWhole] = useState(text.length <= CUT_AT)
  return (
    <Row id={id} cut={!whole} className="plugin-sheet-description">
      <span className="plugin-sheet-text" data-test="plugin-sheet-description">{whole ? text : cutShort(text)}</span>
      {!whole && (
        <Button size="small" kind="plain" className="plugin-show-more" onClick={() => setWhole(true)} data-test="plugin-show-more">Show more</Button>
      )}
    </Row>
  )
}

// ---- the host question ---------------------------------------------------------------------------

function HostSheet({ listing, onAnswer }: { listing: HostListing; onAnswer: (yes: boolean) => void }): React.JSX.Element {
  return (
    <Modal title="Fetch a plugin" size="md" compact snug onClose={() => onAnswer(false)} className="plugin-question"
      actions={<>
        <Button kind="outline" onClick={() => onAnswer(false)} data-test="plugin-no">Don’t fetch</Button>
        <Button kind="filled" onClick={() => onAnswer(true)} data-test="plugin-yes">Yes, fetch from this host</Button>
      </>}>
      <div className="confirm fetch plugin-host" data-test="plugin-host-question">
        <div className="confirm-head">
          <Icon name="globe" className="card-kind" />
          <span className="intent">Fetch</span>
          <code className="path">{listing.url}</code>
        </div>
        <p className="permission-scope fetch-host">
          <strong>Talking to:</strong> <code data-test="plugin-host">{listing.host}</code>
        </p>
        <p className="permission-scope plugin-host-note">
          Nothing has been fetched. A yes starts <code>git</code> to fetch this source, and then shows you everything the
          plugin holds before anything is installed. A no starts nothing and writes nothing.
        </p>
      </div>
    </Modal>
  )
}

// ---- the install and update sheets ---------------------------------------------------------------

/** The frame both sheets share: the untrusted margin, the reading count and the two answers. */
function Sheet({ title, intent, name, yes, no, onAnswer, children, after }: {
  title: string
  intent: string
  name: string
  yes: string
  no: string
  onAnswer: (yes: boolean) => void
  children: React.ReactNode
  after: React.ReactNode
}): React.JSX.Element {
  const { gate, ...reading } = useReadingGate()
  const ready = reading.total > 0 && reading.read === reading.total
  return (
    <Modal title={title} size="lg" snug onClose={() => onAnswer(false)} className="plugin-question"
      subtitle="Everything below was written by the plugin. Read every row before you answer."
      actions={<>
        {!ready && <span className="plugin-sheet-progress modal-leading" role="status" data-test="plugin-reading">{readingHint(reading)}</span>}
        <Button kind="outline" onClick={() => onAnswer(false)} data-test="plugin-no">{no}</Button>
        <Button kind="filled" isDisabled={!ready} onClick={() => { if (ready) onAnswer(true) }} data-test="plugin-yes">{yes}</Button>
      </>}>
      <GateContext.Provider value={gate}>
        {/* Only this scrolls, so the head and the answers stay where they are. */}
        <div className="plugin-sheet-scroll" data-test="plugin-sheet-scroll">
          <div className="confirm untrusted plugin-sheet" data-test="plugin-sheet">
            <div className="confirm-head">
              <Icon name="browser-extensions" className="card-kind" />
              <span className="intent">{intent}</span>
              <span className="path plugin-sheet-name">{name}</span>
              <Label mode="loud" color="yellow" className="trust-label"><Icon name="warning-triangle-filled" slot="icon-before" />Untrusted source</Label>
            </div>
            {children}
          </div>
          {after}
        </div>
      </GateContext.Provider>
    </Modal>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }): React.JSX.Element {
  return (
    <section className="plugin-sheet-section" data-test="plugin-sheet-section">
      <h3>{title}</h3>
      <ul className="plugin-sheet-rows">{children}</ul>
    </section>
  )
}

function Fact({ id, label, value, test }: { id: string; label: string; value: React.ReactNode; test?: string }): React.JSX.Element {
  return (
    <Row id={id} label={label} className="plugin-sheet-fact">
      <span data-test={test}>{value}</span>
    </Row>
  )
}

/** Every word of an argv, each whole and on its own. */
function Argv({ words }: { words: string[] }): React.JSX.Element {
  return (
    <ol className="plugin-argv" data-test="plugin-argv">
      {words.map((word, index) => <li key={index}><code>{word}</code></li>)}
    </ol>
  )
}

const REACH_WORD: Record<Reach['kind'], string> = { command: 'Starts', host: 'Reaches host', file: 'Reads', directory: 'Runs in' }

function Reaches({ id, reaches }: { id: string; reaches: Reach[] }): React.JSX.Element {
  return <>{reaches.map((reach, index) => (
    <Row key={index} id={`${id}.reach${index}`} label={REACH_WORD[reach.kind]} className="plugin-sheet-reach">
      <code data-test="plugin-reach">{reach.untrusted}</code>
    </Row>
  ))}</>
}

function Shadowed({ by }: { by: string | null }): React.ReactNode {
  return by !== null && <span className="plugin-sheet-note">Shadowed by <code>{by}</code>, which keeps the name</span>
}

const skillName = (offer: SkillOffer): React.ReactNode => <>
  <code data-test="plugin-skill">{offer.untrusted.name}</code>
  <Shadowed by={offer.shadowedBy} />
</>

function Skill({ id, offer }: { id: string; offer: SkillOffer }): React.JSX.Element {
  return <>
    <Row id={id} className="plugin-sheet-offer">{skillName(offer)}</Row>
    {offer.untrusted.description !== '' && <Description id={`${id}.description`} text={offer.untrusted.description} />}
  </>
}

/** One side of a changed skill, under the head that names it. */
function SkillSide({ id, offer }: { id: string; offer: SkillOffer }): React.JSX.Element {
  return offer.untrusted.description !== ''
    ? <Description id={`${id}.description`} text={offer.untrusted.description} />
    : <Row id={id} className="plugin-sheet-note">No description.</Row>
}

const definitionName = (offer: DefinitionOffer): React.ReactNode => <>
  <code data-test="plugin-definition">{offer.untrusted.name}</code>
  <Shadowed by={offer.shadowedBy} />
</>

function Definition({ id, offer }: { id: string; offer: DefinitionOffer }): React.JSX.Element {
  return (
    <Row id={id} className="plugin-sheet-offer">
      <code data-test="plugin-definition">{offer.untrusted.name}</code>
      <span className="plugin-sheet-note">{offer.kind}</span>
      <Shadowed by={offer.shadowedBy} />
    </Row>
  )
}

function Hook({ id, hook }: { id: string; hook: HookRow }): React.JSX.Element {
  const { moment, tool, watches, argv } = hook.untrusted
  return <>
    <Row id={id} label="On" className="plugin-sheet-hook">
      <code data-test="plugin-hook-moment">{moment}</code>
      {tool !== null && <span className="plugin-sheet-note"> after <code>{tool}</code></span>}
      {watches !== null && <span className="plugin-sheet-note"> watching <code>{watches}</code></span>}
      {hook.timeoutSecs !== null && <span className="plugin-sheet-note"> stopped after {hook.timeoutSecs} s</span>}
    </Row>
    <Row id={`${id}.argv`} label="Runs" className="plugin-sheet-argv"><Argv words={argv} /></Row>
    <Reaches id={id} reaches={hook.reaches} />
  </>
}

function Server({ id, server }: { id: string; server: ServerRow }): React.JSX.Element {
  const { alias, transport, argv, url, variables, stored } = server.untrusted
  return <>
    <Row id={id} className="plugin-sheet-server">
      <code data-test="plugin-server-alias">{alias}</code>
      <span className="plugin-sheet-note">{transport}</span>
      {server.neverStarted && (
        <Label mode="outline" color="red" data-test="plugin-never-started">
          {server.tooLong ? 'Never started: the name is too long once the plugin’s name is put in front' : 'Never started: this entry cannot be used'}
        </Label>
      )}
    </Row>
    {argv.length > 0 && <Row id={`${id}.argv`} label="Runs" className="plugin-sheet-argv"><Argv words={argv} /></Row>}
    {url !== null && <Row id={`${id}.url`} label="Address"><code>{url}</code></Row>}
    {variables.length > 0 && <Row id={`${id}.variables`} label="Variables"><span className="plugin-sheet-words">{variables.map((name, index) => <code key={index}>{name}</code>)}</span></Row>}
    {stored.map((pair, index) => <Row key={index} id={`${id}.stored${index}`} label="Sets"><code>{pair}</code></Row>)}
    <Reaches id={id} reaches={server.reaches} />
  </>
}

function passedOverRow(id: string, over: PassedOver | null): React.ReactNode {
  if (!over || (over.entries === 0 && over.keys === 0)) return null
  return (
    <Row id={id} className="plugin-sheet-fault">
      The hooks file has {over.entries} entr{over.entries === 1 ? 'y' : 'ies'} and {over.keys} key{over.keys === 1 ? '' : 's'} that
      cannot be used. They are passed over and not shown.
    </Row>
  )
}

function Hooks({ id, part }: { id: string; part: HookPart }): React.JSX.Element {
  return (
    <Section title="Hooks">
      {part.fault !== null
        ? <Row id={`${id}.fault`} className="plugin-sheet-fault">The hooks file cannot be read, so no hook is listed: {part.fault}</Row>
        : part.rows.length === 0
          ? <Row id={`${id}.none`}>The hooks file names no hook.</Row>
          : part.rows.map((hook, index) => <Hook key={index} id={`${id}.${index}`} hook={hook} />)}
      {passedOverRow(`${id}.over`, part.passedOver)}
    </Section>
  )
}

function Servers({ id, part }: { id: string; part: ServerPart }): React.JSX.Element {
  return (
    <Section title="MCP servers">
      {part.fault !== null
        ? <Row id={`${id}.fault`} className="plugin-sheet-fault">The servers file cannot be read, so no server is listed: {part.fault}</Row>
        : part.rows.length === 0
          ? <Row id={`${id}.none`}>The servers file names no server.</Row>
          : part.rows.map((server, index) => <Server key={index} id={`${id}.${index}`} server={server} />)}
    </Section>
  )
}

function Skipped({ paths }: { paths: string[] }): React.ReactNode {
  if (paths.length === 0) return null
  return (
    <Section title="Skipped">
      {paths.map((path, index) => <Row key={index} id={`skipped.${index}`}><code>{path}</code> is not copied.</Row>)}
    </Section>
  )
}

/** What a yes does not do, and what the desktop does with a plugin's hooks. bravebot's own words. */
function WhatYesDoes({ verb, hooks, homeSourcesOnly }: { verb: string; hooks: boolean; homeSourcesOnly: boolean }): React.JSX.Element {
  return (
    <div className="plugin-sheet-after">
      <p>
        A yes {verb} every part listed. It does not make a hook run or a server reachable: each asks you on its first use.
      </p>
      {hooks && <p data-test="plugin-hooks-note">Plugin hooks run only in terminal sessions. Conversations in this app do not run them.</p>}
      {homeSourcesOnly && <p>Shadowing was checked against the skills and definitions in your home directory only, not those of a project.</p>}
    </div>
  )
}

function InstallSheet({ listing, onAnswer }: { listing: InstallListing; onAnswer: (yes: boolean) => void }): React.JSX.Element {
  const u = listing.untrusted
  return (
    <Sheet title="Install a plugin" intent="Install" name={u.name} yes="Yes, install" no="Don’t install" onAnswer={onAnswer}
      after={<WhatYesDoes verb="installs" hooks={!!listing.hooks && listing.hooks.rows.length > 0} homeSourcesOnly={listing.homeSourcesOnly} />}>
      <Section title="Plugin">
        <Fact id="name" label="Name" value={<code>{u.name}</code>} test="plugin-sheet-plugin" />
        <Fact id="version" label="Version" value={<code>{u.version}</code>} test="plugin-sheet-version" />
        <Fact id="source" label="Source" value={<code>{u.source}</code>} />
        {u.requestedRef !== null && <Fact id="ref" label="Ref" value={<code>{u.requestedRef}</code>} />}
        {u.commit !== null && <Fact id="commit" label="Commit" value={<code>{u.commit}</code>} />}
      </Section>
      <Skipped paths={u.skipped} />
      {listing.skills.length > 0 && (
        <Section title="Skills">{listing.skills.map((offer, index) => <Skill key={index} id={`skill.${index}`} offer={offer} />)}</Section>
      )}
      {listing.definitions.length > 0 && (
        <Section title="Agent definitions">{listing.definitions.map((offer, index) => <Definition key={index} id={`definition.${index}`} offer={offer} />)}</Section>
      )}
      {listing.hooks && <Hooks id="hooks" part={listing.hooks} />}
      {listing.servers && <Servers id="servers" part={listing.servers} />}
      {listing.skills.length === 0 && listing.definitions.length === 0 && !listing.hooks && !listing.servers && (
        <Section title="Parts"><Row id="empty">The plugin offers nothing.</Row></Section>
      )}
    </Sheet>
  )
}

const CHANGE_WORD = { added: 'Added', removed: 'Removed', changed: 'Changed' } as const

/**
 * Each delta as its sides: a changed part shows what it was and what it is now. With `head`, the
 * part's name sits on the change word's line and `draw` draws the rest of each side.
 */
function Deltas<T>({ id, deltas, draw, head }: {
  id: string
  deltas: Delta<T>[]
  draw: (id: string, value: T) => React.ReactNode
  head?: (value: T) => React.ReactNode
}): React.JSX.Element {
  return <>{deltas.map((delta, index) => {
    const at = `${id}.${index}`
    const word = <span className="plugin-delta-word">{CHANGE_WORD[delta.change]}</span>
    return (
      <li key={index} className={`plugin-delta plugin-delta-${delta.change}`} data-test="plugin-delta">
        {head
          ? <ul className="plugin-sheet-rows"><Row id={`${at}.head`} className="plugin-delta-head">{word}{head(delta.change === 'removed' ? delta.was : delta.now)}</Row></ul>
          : word}
        <ul className="plugin-sheet-rows">
          {delta.change === 'added' && draw(`${at}.now`, delta.now)}
          {delta.change === 'removed' && draw(`${at}.was`, delta.was)}
          {delta.change === 'changed' && <>
            <li className="plugin-delta-side">Was</li>
            {draw(`${at}.was`, delta.was)}
            <li className="plugin-delta-side plugin-delta-now">Now</li>
            {draw(`${at}.now`, delta.now)}
          </>}
        </ul>
      </li>
    )
  })}</>
}

function PartChanges<T>({ id, title, part, draw }: {
  id: string
  title: string
  part: PartDeltas<T>
  draw: (id: string, value: T) => React.ReactNode
}): React.ReactNode {
  if (part.deltas.length === 0 && part.installedFault === null && part.newFault === null && !part.passedOver?.entries) return null
  return (
    <Section title={title}>
      {part.installedFault !== null && <Row id={`${id}.installedFault`} className="plugin-sheet-fault">The installed copy’s file cannot be read: {part.installedFault}</Row>}
      {part.newFault !== null && <Row id={`${id}.newFault`} className="plugin-sheet-fault">The new copy’s file cannot be read, so nothing in it is listed: {part.newFault}</Row>}
      <Deltas id={id} deltas={part.deltas} draw={draw} />
      {passedOverRow(`${id}.over`, part.passedOver)}
    </Section>
  )
}

const FILE_WORD: Record<FileChange['change'], string> = { added: 'Added', removed: 'Removed', changed: 'Changed' }

/** A value that changes, on two lines: the old one, then "to" and the new one. Each wraps in its line. */
function Change({ was, now }: { was: string; now: string }): React.JSX.Element {
  return (
    <span className="plugin-change">
      <code>{was}</code>
      <span>to <code>{now}</code></span>
    </span>
  )
}

function UpdateSheet({ listing, onAnswer }: { listing: UpdateListing; onAnswer: (yes: boolean) => void }): React.JSX.Element {
  const u = listing.untrusted
  const drawSkill = useCallback((id: string, offer: SkillOffer) => <SkillSide id={id} offer={offer} />, [])
  const drawDefinition = useCallback((id: string, offer: DefinitionOffer) => <Row id={id} label="Kind">{offer.kind}</Row>, [])
  const drawHook = useCallback((id: string, hook: HookRow) => <Hook id={id} hook={hook} />, [])
  const drawServer = useCallback((id: string, server: ServerRow) => <Server id={id} server={server} />, [])
  const nothing = listing.skills.length === 0 && listing.definitions.length === 0
    && !listing.hooks?.deltas.length && !listing.servers?.deltas.length
  return (
    <Sheet title="Update a plugin" intent="Update" name={u.name} yes="Yes, update" no="Keep it as it is" onAnswer={onAnswer}
      after={<WhatYesDoes verb="replaces the installed copy with" hooks={!!listing.hooks?.deltas.length} homeSourcesOnly={listing.homeSourcesOnly} />}>
      <Section title="Plugin">
        <Fact id="name" label="Name" value={<code>{u.name}</code>} test="plugin-sheet-plugin" />
        <Fact id="version" label="Version" value={<><code>{u.installedVersion}</code> to <code>{u.newVersion}</code></>} test="plugin-sheet-versions" />
        <Fact id="source" label="Source" value={<code>{u.source}</code>} />
        {u.installedSource !== u.source && <Fact id="installedSource" label="Installed from" value={<code>{u.installedSource}</code>} />}
        {u.requestedRef !== null && <Fact id="ref" label="Ref" value={<code>{u.requestedRef}</code>} />}
        {(u.installedCommit !== null || u.newCommit !== null) && (
          <Fact id="commit" label="Commit" value={<Change was={u.installedCommit ?? 'none'} now={u.newCommit ?? 'none'} />} />
        )}
        <Fact id="digest" label="Digest" value={<Change was={listing.installedDigest} now={listing.newDigest} />} />
        <Fact id="enabled" label="State" value={listing.enabled ? 'On, and stays on after the update' : 'Off, and stays off after the update'} />
        {!listing.before.readable && (
          <Row id="unreadable" className="plugin-sheet-fault">The installed copy cannot be read, so everything below is listed as added.</Row>
        )}
      </Section>
      <Skipped paths={u.skipped} />
      {listing.skills.length > 0 && <Section title="Skills"><Deltas id="skill" deltas={listing.skills} head={skillName} draw={drawSkill} /></Section>}
      {listing.definitions.length > 0 && <Section title="Agent definitions"><Deltas id="definition" deltas={listing.definitions} head={definitionName} draw={drawDefinition} /></Section>}
      {listing.hooks && <PartChanges id="hooks" title="Hooks" part={listing.hooks} draw={drawHook} />}
      {listing.servers && <PartChanges id="servers" title="MCP servers" part={listing.servers} draw={drawServer} />}
      {nothing && <Section title="Parts"><Row id="same">No skill, definition, hook or server changes.</Row></Section>}
      <Section title="Files">
        {listing.files === null
          ? <Row id="files.unknown" className="plugin-sheet-fault">The installed copy could not be compared, so the files that differ are not known.</Row>
          : listing.files.length === 0
            ? <Row id="files.none">No file differs.</Row>
            : listing.files.map((file, index) => (
              <Row key={index} id={`file.${index}`} className="plugin-sheet-file">
                <span className={`plugin-file-change plugin-delta-${file.change}`}>{FILE_WORD[file.change]}</span>
                <code data-test="plugin-file">{file.untrusted}</code>
              </Row>
            ))}
      </Section>
    </Sheet>
  )
}
