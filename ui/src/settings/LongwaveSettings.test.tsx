// @vitest-environment jsdom
import { useState } from 'react'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { LongwaveSettings } from './LongwaveSettings'
import type { Settings } from '../types'

const api = vi.hoisted(() => ({
  setLongwaveToken: vi.fn(), clearLongwaveToken: vi.fn(), getLongwaveLogbooks: vi.fn(),
}))
vi.mock('../api', () => api)
vi.mock('../toast', () => ({ pushToast: vi.fn() }))

function Fixture({ remote = false }: { remote?: boolean }) {
  const [form, setForm] = useState({
    longwaveUrl: 'https://log.example.invalid', longwaveLogbookId: '', longwaveUpload: false,
  } as Settings)
  return <><LongwaveSettings form={form} remote={remote} onChange={patch => setForm(f => ({ ...f, ...patch }))} />
    <output data-testid="form">{JSON.stringify(form)}</output></>
}

beforeEach(() => {
  vi.resetAllMocks()
  api.getLongwaveLogbooks.mockResolvedValue([
    { id: 'book-a', name: 'Home', operator_callsign: 'N0CALL', contact_count: 0 },
  ])
})
afterEach(cleanup)

describe('Longwave inside logging settings', () => {
  it('does not contact Longwave or store a token in Settings on mount', () => {
    render(<Fixture />)
    expect(api.getLongwaveLogbooks).not.toHaveBeenCalled()
    expect(api.setLongwaveToken).not.toHaveBeenCalled()
    expect(screen.getByRole('switch', { name: 'Forward logged QSOs' }).getAttribute('aria-checked')).toBe('false')
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('saves a token through the keychain command and clears the password input', async () => {
    render(<Fixture />)
    fireEvent.change(screen.getByLabelText('Client token'), { target: { value: 'ephemeral-test-input' } })
    fireEvent.click(screen.getByRole('button', { name: 'Set token' }))
    await waitFor(() => expect(api.setLongwaveToken).toHaveBeenCalledWith('ephemeral-test-input'))
    await waitFor(() => expect((screen.getByLabelText('Client token') as HTMLInputElement).value).toBe(''))
    expect(screen.getByTestId('form').textContent).not.toContain('ephemeral-test-input')
  })

  it('tests the edited URL and selects the sole available logbook without a dialog', async () => {
    render(<Fixture />)
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }))
    await waitFor(() => expect(screen.getByRole('option', { name: 'Home (N0CALL)' })).toBeTruthy())
    expect(api.getLongwaveLogbooks).toHaveBeenCalledWith('https://log.example.invalid')
    expect(JSON.parse(screen.getByTestId('form').textContent!).longwaveLogbookId).toBe('book-a')
    fireEvent.click(screen.getByRole('switch', { name: 'Forward logged QSOs' }))
    expect(JSON.parse(screen.getByTestId('form').textContent!).longwaveUpload).toBe(true)
  })

  it('discards a late logbook response after changing the server', async () => {
    let resolve!: (value: unknown) => void
    api.getLongwaveLogbooks.mockImplementation(() => new Promise(r => { resolve = r }))
    render(<Fixture />)
    fireEvent.click(screen.getByRole('button', { name: 'Test connection' }))
    fireEvent.change(screen.getByLabelText('Server URL'), { target: { value: 'https://other.invalid' } })
    resolve([{ id: 'old-book', name: 'Old server', operator_callsign: 'N0CALL', contact_count: 0 }])
    await waitFor(() => expect(screen.getByRole('button', { name: 'Test connection' })).toBeTruthy())
    expect(screen.queryByRole('option', { name: 'Old server (N0CALL)' })).toBeNull()
    expect(JSON.parse(screen.getByTestId('form').textContent!).longwaveLogbookId).toBe('')
  })

  it('keeps configuration and credentials disabled for remote users', () => {
    render(<Fixture remote />)
    expect((screen.getByLabelText('Server URL') as HTMLInputElement).disabled).toBe(true)
    expect((screen.getByRole('button', { name: 'Test connection' }) as HTMLButtonElement).disabled).toBe(true)
    expect((screen.getByLabelText('Client token') as HTMLInputElement).disabled).toBe(true)
  })
})
