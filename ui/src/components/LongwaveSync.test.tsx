// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { LongwaveSync } from './LongwaveSync'

const mocks = vi.hoisted(() => ({ sync: vi.fn(), refresh: vi.fn(), toast: vi.fn() }))
vi.mock('../api', () => ({ syncLongwaveLogbook: mocks.sync }))
vi.mock('../features/logSource', () => ({ logSource: () => ({ refresh: mocks.refresh }) }))
vi.mock('../toast', () => ({ pushToast: mocks.toast }))
beforeEach(() => vi.resetAllMocks())
afterEach(cleanup)

it('syncs once while busy, refreshes shared history, and hands back the import run', async () => {
  let finish!: (stats: unknown) => void
  mocks.sync.mockReturnValue(new Promise(resolve => { finish = resolve }))
  const imported = vi.fn()
  render(<LongwaveSync onImported={imported} />)
  const button = screen.getByRole('button', { name: 'Sync Longwave' })
  fireEvent.click(button)
  fireEvent.click(button)
  expect(mocks.sync).toHaveBeenCalledTimes(1)
  expect(mocks.sync).toHaveBeenCalledWith()
  expect((button as HTMLButtonElement).disabled).toBe(true)
  const stats = { added: 2, skipped: 1, updated: 3, total: 6, run: 'sync-a' }
  finish(stats)
  await waitFor(() => expect(imported).toHaveBeenCalledWith(stats))
  expect(mocks.refresh).toHaveBeenCalledTimes(1)
  expect((button as HTMLButtonElement).disabled).toBe(false)
})

it('reports a failed sync, keeps history, and allows an explicit retry', async () => {
  mocks.sync.mockRejectedValue(new Error('Destination unavailable'))
  render(<LongwaveSync />)
  fireEvent.click(screen.getByRole('button', { name: 'Sync Longwave' }))
  await waitFor(() => expect(mocks.toast).toHaveBeenCalledWith(expect.stringContaining('Destination unavailable'), 'error'))
  expect(mocks.refresh).not.toHaveBeenCalled()
  expect((screen.getByRole('button', { name: 'Sync Longwave' }) as HTMLButtonElement).disabled).toBe(false)
})
