// @vitest-environment jsdom
import { afterEach, expect, it } from 'vitest'
import * as api from './api'
import type { LoggedQso } from './types'

afterEach(() => { delete window.__TAURI_INTERNALS__ })

it('syncs the saved destination with no form arguments and pushes the stored record', async () => {
  const calls: unknown[] = []
  window.__TAURI_INTERNALS__ = { invoke: async <T,>(cmd: string, args?: Record<string, unknown>) => {
    calls.push([cmd, args])
    return {} as T
  } }
  const record = { id: '~0000000000000001', call: 'N0CALL' } as LoggedQso
  await api.syncLongwaveLogbook()
  await api.longwavePushQso(record)
  expect(calls).toEqual([
    ['longwave_sync_logbook', undefined],
    ['longwave_push_qso', { record }],
  ])
})
