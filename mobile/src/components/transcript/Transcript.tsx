import { memo, useCallback, useEffect, useMemo, useRef } from 'react'
import { FlatList, StyleSheet, View, type NativeScrollEvent, type NativeSyntheticEvent } from 'react-native'
import type { Row, Status } from '@brave/agent-client'
import { readingWidth, space } from '../../theme'
import { Meta } from '../primitives'
import { latestTurn, turnIsRunning } from '../../calls'
import type { Decision } from '../../session/controller'
import { RowView } from './RowView'

const NEAR_END = 120

function lastPromptId(rows: readonly Row[]): number {
  for (let at = rows.length - 1; at >= 0; at--) if (rows[at]!.kind === 'prompt') return rows[at]!.id
  return 0
}

export const Transcript = memo(function Transcript({
  rows,
  status,
  ended = false,
  decisions,
  empty,
}: {
  rows: readonly Row[]
  status: Status
  /** The view ended, so whatever its last status was, nothing is working. */
  ended?: boolean
  decisions: ReadonlyMap<number, Decision>
  empty: React.ReactElement | null
}) {
  const list = useRef<FlatList<Row>>(null)
  const following = useRef(true)
  const lastPrompt = useRef(0)
  const running = turnIsRunning(status, ended)
  const listState = useMemo(() => ({ running, decisions }), [running, decisions])
  const newest = latestTurn(rows)

  // A message the person has just sent is the one thing they came to see, wherever they had scrolled to.
  useEffect(() => {
    const sent = lastPromptId(rows)
    if (sent > lastPrompt.current) following.current = true
    lastPrompt.current = sent
  }, [rows])

  const onScroll = useCallback((event: NativeSyntheticEvent<NativeScrollEvent>) => {
    const { contentOffset, contentSize, layoutMeasurement } = event.nativeEvent
    following.current = contentSize.height - (contentOffset.y + layoutMeasurement.height) < NEAR_END
  }, [])

  const onContentSizeChange = useCallback(() => {
    if (following.current) list.current?.scrollToEnd({ animated: false })
  }, [])

  return (
    <FlatList
      ref={list}
      testID="transcript"
      data={rows}
      extraData={listState}
      keyExtractor={(row) => String(row.id)}
      renderItem={({ item, index }) => (
        <View style={[styles.item, item.kind === 'prompt' && index > 0 && styles.turn]}>
          <RowView row={item} decision={item.kind === 'approval' ? decisions.get(item.id) : undefined} live={running && item.turn === newest} />
        </View>
      )}
      ListEmptyComponent={empty ?? undefined}
      ListFooterComponent={
        <View style={styles.footer}>{!ended && status === 'running' && <Meta testID="working">Working…</Meta>}</View>
      }
      onScroll={onScroll}
      scrollEventThrottle={64}
      onContentSizeChange={onContentSizeChange}
      onLayout={onContentSizeChange}
      contentContainerStyle={styles.content}
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="on-drag"
      style={styles.list}
    />
  )
})

const styles = StyleSheet.create({
  list: { flex: 1 },
  content: { paddingHorizontal: space.xl, paddingTop: space.l, flexGrow: 1, width: '100%', maxWidth: readingWidth, alignSelf: 'center' },
  item: { paddingVertical: space.s + 2 },
  // 24 between turns: this plus the 6 under the row above.
  turn: { paddingTop: space.xxl - 6 },
  footer: { height: 28, justifyContent: 'center' },
})
