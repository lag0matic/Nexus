// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ObserverAccess } from './ObserverAccess'

vi.mock('./ObserverPanels', () => ({ ObserverPanels: () => <div>Connected panels</div> }))
afterEach(() => { cleanup(); vi.unstubAllGlobals(); vi.restoreAllMocks(); localStorage.clear(); sessionStorage.clear(); delete window.__NEXUS_OBSERVER_ACCESS__ })

describe('typed browser access', () => {
  it('remembers durable pairing across reloads and new tabs without saving the access code', async () => {
    const access = 'a'.repeat(64)
    const fetcher = vi.fn().mockResolvedValue(Response.json({ access, persistent: true }))
    vi.stubGlobal('fetch', fetcher)
    const view = render(<ObserverAccess />)
    expect(fetcher).not.toHaveBeenCalled()
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '1234 5678' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await screen.findByText('Connected panels')
    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(fetcher.mock.calls[0][0]).toBe('/observer/pair')
    expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({ code: '12345678' })
    expect(window.__NEXUS_OBSERVER_ACCESS__).toBe(access)
    expect(localStorage.getItem('nexus.browser-panels.access')).toBe(access)
    expect(Object.values(localStorage).some(v => v.includes('12345678'))).toBe(false)
    view.unmount()
    delete window.__NEXUS_OBSERVER_ACCESS__
    sessionStorage.clear()
    render(<ObserverAccess />)
    expect(screen.getByText('Connected panels')).toBeTruthy()
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('uses only refresh-safe tab storage when remember is unchecked or the server cannot persist', async () => {
    const access = 'b'.repeat(64)
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access, persistent: true })))
    const view = render(<ObserverAccess />)
    fireEvent.click(screen.getByLabelText('Remember this browser'))
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '12345678' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await screen.findByText('Connected panels')
    expect(localStorage.length).toBe(0)
    expect(sessionStorage.getItem('nexus.browser-panels.access')).toBe(access)
    view.unmount()
    delete window.__NEXUS_OBSERVER_ACCESS__
    render(<ObserverAccess />)
    expect(screen.getByText('Connected panels')).toBeTruthy()
  })

  it('keeps pairing available even if the browser denies storage', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('denied') })
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new Error('denied') })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access: 'c'.repeat(64), persistent: true })))
    render(<ObserverAccess />)
    fireEvent.change(screen.getByLabelText('Access code'), { target: { value: '12345678' } })
    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await screen.findByText('Connected panels')
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
