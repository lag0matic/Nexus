// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { BrowserPanelShare } from './BrowserPanelShare'
import { observerPanelAction } from '../api'

vi.mock('../api', () => ({ observerPanelAction: vi.fn() }))
afterEach(() => { cleanup(); delete window.__TAURI_INTERNALS__; vi.clearAllMocks() })

describe('local browser sharing control', () => {
  it('is absent in a plain browser', () => {
    const { container } = render(<BrowserPanelShare panel="pota" />)
    expect(container.querySelector('button')).toBeNull()
    expect(observerPanelAction).not.toHaveBeenCalled()
  })

  it('starts only on an explicit local press and revokes sharing on Stop', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
    const off = { running: false, address: '192.168.1.10', port: 7375, links: {} }
    const on = { ...off, running: true, accessCode: '12345678', links: { pota: 'http://192.168.1.10:7375' } }
    vi.mocked(observerPanelAction).mockResolvedValueOnce(off).mockResolvedValueOnce(on).mockResolvedValueOnce(off)
    render(<BrowserPanelShare panel="pota" />)
    expect(observerPanelAction).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Share this panel in a browser' }))
    await waitFor(() => expect((screen.getByRole('button', { name: 'Start sharing' }) as HTMLButtonElement).disabled).toBe(false))
    expect(observerPanelAction).toHaveBeenLastCalledWith({ kind: 'status' })
    fireEvent.click(screen.getByRole('button', { name: 'Start sharing' }))
    await screen.findByDisplayValue(on.links.pota)
    expect(screen.getByLabelText('Browser address')).toHaveProperty('value', on.links.pota)
    expect(screen.getByLabelText('Access code')).toHaveProperty('value', '1234 5678')
    expect(observerPanelAction).toHaveBeenLastCalledWith({ kind: 'start', address: off.address, port: 7375 })
    fireEvent.click(screen.getByRole('button', { name: 'Stop sharing' }))
    await waitFor(() => expect(observerPanelAction).toHaveBeenLastCalledWith({ kind: 'stop' }))
    await waitFor(() => expect(screen.queryByDisplayValue(on.links.pota)).toBeNull())
    expect(screen.queryByLabelText('Access code')).toBeNull()
  })
})
