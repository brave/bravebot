import { useState } from 'react'
import { StyleSheet, Text, TextInput, View } from 'react-native'
import { Composer } from '../components/Composer'
import { ConversationCard } from '../components/ConversationCard'
import { Body, Button, Meta } from '../components/primitives'
import { TopBar } from '../components/TopBar'
import type { Connect, Connection } from '../targets'
import { radius, space, type, useTheme } from '../theme'

const REASON = 'No target is connected.'

interface NoTargetProps {
  /** Reaches a target at a port and code the person types. Without it, nothing is offered. */
  connect?: Connect
  onConnected?: (connection: Connection) => void
  /** Why the last connection ended, if one did. */
  ended?: string | null
}

const PORT = /^[0-9]{1,5}$/
const CODE = /^[0-9]{6}$/

/** What the app shows when a build offers no target: the same frame, with nothing to run a session on. */
export function NoTargetScreen({ connect, onConnected, ended = null }: NoTargetProps) {
  const theme = useTheme()
  return (
    <View style={[styles.screen, { backgroundColor: theme.surface.app }]} testID="no-target">
      <TopBar title="Bravebot" />
      <ConversationCard>
        <View style={styles.body}>
          <Text accessibilityRole="header" style={[type.hero, styles.heading, { color: theme.ink.primary }]}>
            Nothing is connected
          </Text>
          {connect === undefined ? (
            <Body style={[styles.lede, { color: theme.ink.secondary }]}>
              A session runs on a target, such as a host machine. This build has none, so there is nothing to start.
            </Body>
          ) : (
            <ConnectForm connect={connect} onConnected={onConnected} ended={ended} />
          )}
        </View>
        <Composer
          draft=""
          onDraft={() => undefined}
          status="idle"
          ended={REASON}
          sending={false}
          workspace="No workspace"
          onSend={() => undefined}
          onCancel={() => undefined}
          error={null}
        />
      </ConversationCard>
    </View>
  )
}

function ConnectForm({ connect, onConnected, ended }: { connect: Connect; onConnected?: (connection: Connection) => void; ended: string | null }) {
  const theme = useTheme()
  const [port, setPort] = useState('')
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const number = Number(port)
  const ready = PORT.test(port) && number >= 1 && number <= 65535 && CODE.test(code) && !busy

  const submit = () => {
    setBusy(true)
    setProblem(null)
    connect(number, code).then(
      (connection) => {
        setBusy(false)
        onConnected?.(connection)
      },
      (error: unknown) => {
        setBusy(false)
        // A wrong code ends that relay, so the next try needs the code of a new launch.
        setCode('')
        setProblem(error instanceof Error ? error.message : 'The connection could not be made.')
      },
    )
  }

  const field = [type.body, styles.field, { color: theme.ink.primary, borderColor: theme.border.strong, backgroundColor: theme.surface.raised }]
  const notice = problem ?? ended
  return (
    <View style={styles.form} testID="connect-form">
      <Body style={[styles.lede, { color: theme.ink.secondary }]}>
        This development build can reach bravebot-host through the development relay. Forward the relay's port to this device, then enter the port and code the relay shows.
      </Body>
      <View style={styles.fields}>
        <View style={styles.labelled}>
          <Meta style={{ color: theme.ink.secondary }}>Port</Meta>
          <TextInput testID="connect-port" accessibilityLabel="Port" keyboardType="number-pad" maxLength={5} value={port} onChangeText={setPort} editable={!busy} style={field} />
        </View>
        <View style={styles.labelled}>
          <Meta style={{ color: theme.ink.secondary }}>Code</Meta>
          <TextInput
            testID="connect-code"
            accessibilityLabel="Code"
            keyboardType="number-pad"
            maxLength={6}
            autoComplete="off"
            value={code}
            onChangeText={setCode}
            editable={!busy}
            style={field}
          />
        </View>
      </View>
      <Button testID="connect" label={busy ? 'Connecting…' : 'Connect'} variant="primary" disabled={!ready} onPress={submit} />
      {notice !== null && (
        <Body testID="connect-problem" style={[styles.lede, { color: problem === null ? theme.ink.secondary : theme.status.error }]} accessibilityLiveRegion="polite">
          {notice}
        </Body>
      )}
    </View>
  )
}

const styles = StyleSheet.create({
  screen: { flex: 1 },
  body: { flex: 1, paddingTop: space.xxl * 2, paddingHorizontal: space.xl, gap: space.l, alignItems: 'center' },
  heading: { textAlign: 'center' },
  lede: { textAlign: 'center' },
  form: { gap: space.l, alignItems: 'center', alignSelf: 'stretch' },
  fields: { flexDirection: 'row', gap: space.l },
  labelled: { gap: space.s },
  field: { width: 120, borderWidth: 1, borderRadius: radius.control, paddingHorizontal: space.l, paddingVertical: space.m, fontVariant: ['tabular-nums'] },
})
