import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { StatusBar, StyleSheet, View } from 'react-native'
import { SafeAreaProvider } from 'react-native-safe-area-context'
import type { AgentSession, SessionSummary, ViewState } from '@brave/agent-client'
import { Drawer } from './components/Drawer'
import { Body, Button, Meta } from './components/primitives'
import { ChatScreen } from './screens/ChatScreen'
import { SessionController, describeError } from './session/controller'
import { NoTargetScreen } from './screens/NoTargetScreen'
import type { Target } from './targets'
import { displayString } from './text/displayText'
import { workspaceName } from './workspaces'
import { space, useTheme } from './theme'

/** What a session's row in the list shows, so a change to anything else need not fetch the list again. */
function rowFacts(view: ViewState): string {
  return [view.status, view.pending?.request ?? '', view.ended !== null, view.rows.some((row) => row.kind === 'prompt')].join('|')
}

function Shell({ target }: { target: Target }) {
  const theme = useTheme()
  const client = useMemo(() => target.create(), [target])
  // Names are drawn as plain text in several places, so characters that hide or reorder text are written out once, here.
  const workspaces = useMemo(() => client.workspaces().map((candidate) => ({ id: candidate.id, name: displayString(candidate.name) })), [client])
  const [sessions, setSessions] = useState<readonly SessionSummary[]>([])
  const [current, setCurrent] = useState<SessionController | null>(null)
  const [drawer, setDrawer] = useState(false)
  const [workspace, setWorkspace] = useState(workspaces[0]?.id ?? '')
  // Where the session on screen runs. The picker above only chooses where the next one starts.
  const [currentWorkspace, setCurrentWorkspace] = useState(workspace)
  const [problem, setProblem] = useState<string | null>(null)
  const controllers = useRef(new Map<AgentSession, SessionController>())
  const latest = useRef(0)
  const listed = useRef(0)
  const applied = useRef(0)
  const gone = useRef(false)

  const refresh = useCallback(() => {
    // An answer older than one already shown is out of date. A newer request that fails leaves the last list that arrived.
    const ticket = ++listed.current
    client.listSessions().then(
      (list) => {
        if (gone.current || ticket < applied.current) return
        applied.current = ticket
        setSessions(list)
      },
      () => undefined,
    )
  }, [client])

  const controllerFor = useCallback(
    (session: AgentSession): SessionController => {
      let controller = controllers.current.get(session)
      if (controller === undefined) {
        controller = new SessionController(session, refresh)
        controllers.current.set(session, controller)
      }
      return controller
    },
    [refresh],
  )

  // Only the session picked last takes the screen. One made for an earlier pick that comes back late is closed.
  const choose = useCallback(
    async (load: () => Promise<AgentSession>, discardIfLate: boolean, madeIn: string) => {
      const ticket = ++latest.current
      setProblem(null)
      try {
        const session = await load()
        if (gone.current || ticket !== latest.current) {
          if (discardIfLate) void session.close().catch(() => undefined)
          return
        }
        setCurrent(controllerFor(session))
        setCurrentWorkspace(madeIn)
        setDrawer(false)
        refresh()
      } catch (error) {
        if (!gone.current && ticket === latest.current) setProblem(describeError(error))
      }
    },
    [controllerFor, refresh],
  )

  const start = useCallback((id: string) => choose(() => client.createSession({ workspace: id }), true, id), [client, choose])
  const openListed = useCallback(
    (id: string) => {
      // The session on screen keeps its draft and answers, so opening it again only closes the drawer.
      if (current?.session.id === id) {
        // A pick still on its way no longer wins, because the person chose to stay.
        latest.current++
        setProblem(null)
        setDrawer(false)
        return Promise.resolve()
      }
      return choose(() => client.openSession(id), false, sessions.find((candidate) => candidate.id === id)?.workspace ?? workspace)
    },
    [client, choose, current, sessions, workspace],
  )

  // The list follows the session on screen, so a row does not go on saying Working after the turn is done.
  // Only a change to what a row shows asks for a new list, not every row the turn streams in.
  useEffect(() => {
    if (current === null) return undefined
    let shown = rowFacts(current.session.view)
    return current.session.subscribe((view) => {
      const now = rowFacts(view)
      if (now === shown) return
      shown = now
      refresh()
    })
  }, [current, refresh])

  useEffect(() => {
    gone.current = false
    return () => {
      gone.current = true
    }
  }, [])

  useEffect(() => {
    void start(workspace)
    // The first session opens once, for the workspace selected at launch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client])

  // A session with nothing in it is already a new one, so asking for another in the same project only closes the drawer.
  // One waiting on a trust answer is the exception: it may be the one a person is trying to leave.
  const newSession = useCallback(() => {
    if (
      current !== null &&
      current.session.view.rows.length === 0 &&
      current.session.view.ended === null &&
      current.session.view.status !== 'awaiting_trust' &&
      currentWorkspace === workspace
    ) {
      latest.current++
      setProblem(null)
      setDrawer(false)
      return
    }
    void start(workspace)
  }, [current, currentWorkspace, workspace, start])

  const openDrawer = useCallback(() => {
    setProblem(null)
    refresh()
    setDrawer(true)
  }, [refresh])

  const summary = current === null ? undefined : sessions.find((candidate) => candidate.id === current.session.id)

  return (
    <View style={[styles.root, { backgroundColor: theme.surface.app }]}>
      <StatusBar barStyle={theme.scheme === 'dark' ? 'light-content' : 'dark-content'} />
      <View style={styles.content} testID="content" importantForAccessibility={drawer ? 'no-hide-descendants' : 'auto'} accessibilityElementsHidden={drawer}>
        {current === null ? (
          <View style={styles.centered} testID="starting">
            {problem === null ? (
              <Meta>Starting…</Meta>
            ) : (
              <View style={styles.problem}>
                <Body>{problem}</Body>
                <Button label="Try again" onPress={() => void start(workspace)} />
              </View>
            )}
          </View>
        ) : (
          <ChatScreen
            key={current.session.id}
            controller={current}
            target={target.label}
            title={summary?.firstPrompt ?? null}
            suggestions={target.suggestions}
            workspaceName={workspaceName(workspaces, summary?.workspace ?? currentWorkspace)}
            onMenu={openDrawer}
          />
        )}
      </View>
      <Drawer
        open={drawer}
        onClose={() => setDrawer(false)}
        target={{ label: target.label, description: target.description }}
        sessions={sessions}
        workspaces={workspaces}
        workspace={workspace}
        onWorkspace={setWorkspace}
        activeId={current?.session.id ?? null}
        onOpenSession={(id) => void openListed(id)}
        onNewSession={newSession}
        problem={current === null ? null : problem}
      />
    </View>
  )
}

/** The app over the targets a build offers. With none, it shows a blank slate. */
export default function App({ targets = [] }: { targets?: readonly Target[] }) {
  const target = targets[0]
  return <SafeAreaProvider>{target === undefined ? <NoTargetScreen /> : <Shell key={target.id} target={target} />}</SafeAreaProvider>
}

const styles = StyleSheet.create({
  root: { flex: 1 },
  content: { flex: 1 },
  centered: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: space.xl },
  problem: { gap: space.l, alignItems: 'center' },
})
