// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { LongwaveUpload } from './LongwaveUpload'
import { ContactCard } from './ContactCard'
import type { LoggedQso } from '../types'

const mocks = vi.hoisted(() => ({ push: vi.fn(), refresh: vi.fn(), toast: vi.fn() }))
vi.mock('../api', () => ({ longwavePushQso: mocks.push }))
vi.mock('../features/logSource', async importOriginal => ({ ...await importOriginal<object>(), logSource: () => ({ refresh: mocks.refresh }), useLogAnswer: () => null }))
vi.mock('../toast', () => ({ pushToast: mocks.toast }))
vi.mock('./QsoHistory', () => ({ QsoHistory: () => null }))
beforeEach(() => vi.resetAllMocks())
afterEach(cleanup)

const contact = (outcome?: string): LoggedQso => ({
  id: '~0000000000000001', call: 'N0CALL', band: '20m', mode: 'FT8', whenUnix: 1_700_000_000,
  upload: { longwave: outcome ? { outcome, whenUnix: 0 } : null },
}) as LoggedQso
const button = () => screen.getByRole('button', { name: 'Upload to Longwave' }) as HTMLButtonElement

it('does not retain a stale displayed stamp before any local attempt', () => {
  const { rerender } = render(<LongwaveUpload qso={contact('pending')} />)
  expect(button().disabled).toBe(true)
  rerender(<LongwaveUpload qso={contact()} />)
  expect(button().disabled).toBe(false)
  expect(mocks.push).not.toHaveBeenCalled()
})

it.each(['pending', 'accepted', 'duplicate'])('never pushes a stored %s stamp', outcome => {
  render(<LongwaveUpload qso={contact(outcome)} />)
  expect(button().disabled).toBe(true)
  fireEvent.click(button())
  expect(mocks.push).not.toHaveBeenCalled()
})

it.each(['pending', 'accepted', 'duplicate'])('locks after a %s reply, including two rapid clicks', async outcome => {
  let finish!: (report: unknown) => void
  mocks.push.mockReturnValue(new Promise(resolve => { finish = resolve }))
  const q = contact()
  const changed = vi.fn()
  render(<LongwaveUpload qso={q} onChanged={changed} />)
  const upload = button()
  fireEvent.click(upload)
  fireEvent.click(upload)
  expect(mocks.push).toHaveBeenCalledTimes(1)
  expect(mocks.push).toHaveBeenCalledWith(q)
  finish({ dispatched: 1, outcome })
  await waitFor(() => expect(changed).toHaveBeenCalledTimes(1))
  expect(button().disabled).toBe(true)
  fireEvent.click(button())
  expect(mocks.push).toHaveBeenCalledTimes(1)
  expect(mocks.refresh).toHaveBeenCalledTimes(1)
})

it('does not blindly retry a lost reply', async () => {
  mocks.push.mockRejectedValue(new Error('Connection lost'))
  render(<LongwaveUpload qso={contact()} />)
  fireEvent.click(button())
  await waitFor(() => expect(mocks.toast).toHaveBeenCalledWith(expect.stringContaining('Connection lost'), 'error'))
  expect(button().disabled).toBe(true)
  expect(button().title).toBe('Unconfirmed. Sync history and check Longwave before sending again.')
  fireEvent.click(button())
  expect(mocks.push).toHaveBeenCalledTimes(1)
})

it.each(['rejected', 'authfail'])('a refreshed definitive %s stamp resolves an IPC failure', async outcome => {
  mocks.push.mockRejectedValue(new Error('Command refused'))
  const { rerender } = render(<LongwaveUpload qso={contact()} />)
  fireEvent.click(button())
  await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1))
  expect(button().disabled).toBe(true)
  rerender(<LongwaveUpload qso={contact(outcome)} />)
  await waitFor(() => expect(button().disabled).toBe(false))
  fireEvent.click(button())
  await waitFor(() => expect(mocks.push).toHaveBeenCalledTimes(2))
})

it.each([undefined, 'pending', 'accepted', 'duplicate'])('a refreshed %s stamp never permits a blind retry', async outcome => {
  mocks.push.mockRejectedValue(new Error('Reply lost'))
  const { rerender } = render(<LongwaveUpload qso={contact()} />)
  fireEvent.click(button())
  await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1))
  rerender(<LongwaveUpload qso={contact(outcome)} />)
  expect(button().disabled).toBe(true)
  fireEvent.click(button())
  expect(mocks.push).toHaveBeenCalledTimes(1)
})

it('permits retry after a definitive rejection', async () => {
  mocks.push.mockResolvedValue({ dispatched: 1, outcome: 'rejected' })
  render(<LongwaveUpload qso={contact('authfail')} />)
  fireEvent.click(button())
  await waitFor(() => expect(mocks.refresh).toHaveBeenCalledTimes(1))
  expect(button().disabled).toBe(false)
})

it('shows Longwave pending as unconfirmed on the upstream contact card', () => {
  const props = { onClose: vi.fn(), onSwitch: vi.fn(), save: vi.fn(), onChanged: vi.fn(), onReload: vi.fn() }
  const { rerender } = render(<ContactCard open={{ row: contact('pending'), ref: null }} longwaveConfigured {...props} />)
  expect(screen.getByText('Longwave upload')).toBeTruthy()
  expect(screen.getByText('Unconfirmed. Sync history and check Longwave before sending again.')).toBeTruthy()
  expect(button().disabled).toBe(true)
  rerender(<ContactCard open={{ row: contact(), ref: null }} {...props} />)
  expect(screen.queryByRole('button', { name: 'Upload to Longwave' })).toBeNull()
})
