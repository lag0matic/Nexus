// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ObserverAccess } from './ObserverAccess'

vi.mock('./ObserverPanels', () => ({ ObserverPanels: () => <div>Connected panels</div> }))
afterEach(() => { cleanup(); vi.unstubAllGlobals(); delete window.__NEXUS_OBSERVER_ACCESS__ })

describe('typed browser access', () => {
  it('exchanges the displayed code and keeps the secret only in tab memory', async () => {
    const access = 'a'.repeat(64)
    const fetcher = vi.fn().mockResolvedValue(Response.json({ access }))
    vi.stubGlobal('fetch', fetcher)
    const local = vi.spyOn(Storage.prototype, 'setItem')
    render(<ObserverAccess />)
    expect(fetcher).not.toHaveBeenCalled()
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '1234 5678' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await screen.findByText('Connected panels')
    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(fetcher.mock.calls[0][0]).toBe('/observer/pair')
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ code: '12345678' })
    expect(window.__NEXUS_OBSERVER_ACCESS__).toBe(access)
    expect(local).not.toHaveBeenCalled()
    local.mockRestore()
  })

  it('shows a refused code without loading panels or automatically retrying', async () => {
    const fetcher = vi.fn().mockResolvedValue(new Response('Incorrect access code', { status: 403 }))
    vi.stubGlobal('fetch', fetcher)
    render(<ObserverAccess />)
    expect(screen.getByRole('button', { name: 'Connect' })).toHaveProperty('disabled', true)
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '87654321' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await screen.findByRole('alert')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Connect' })).toHaveProperty('disabled', false))
    expect(screen.queryByText('Connected panels')).toBeNull()
    expect(window.__NEXUS_OBSERVER_ACCESS__).toBeUndefined()
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('refuses malformed token responses', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access: 'not-a-session-token' })))
    render(<ObserverAccess />)
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '12345678' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    expect((await screen.findByRole('alert')).textContent).toContain('Invalid access response')
    expect(window.__NEXUS_OBSERVER_ACCESS__).toBeUndefined()
  })
})
