// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { BrowserPanelShare } from './BrowserPanelShare'
import { observerPanelAction } from '../api'
import { useEscStop } from '../useEscStop'

vi.mock('../api', () => ({ observerPanelAction: vi.fn() }))
afterEach(() => { cleanup(); delete window.__TAURI_INTERNALS__; vi.clearAllMocks() })

describe('local browser sharing control', () => {
  it('saves startup consent only on the local checkbox and confirms forgetting separately from Stop', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
    const on = { running: true, persistent: true, automatic: false, address: '192.168.1.10', port: 7375, links: {} }
    vi.mocked(observerPanelAction).mockResolvedValueOnce(on).mockResolvedValueOnce({ ...on, automatic: true })
      .mockResolvedValueOnce({ ...on, running: false, persistent: false })
    render(<BrowserPanelShare panel="pota" />)
    fireEvent.click(screen.getByRole('button', { name: 'Share this panel in a browser' }))
    const checkbox = await screen.findByLabelText('Start browser sharing with Nexus')
    await waitFor(() => expect(checkbox).toHaveProperty('disabled', false))
    expect(checkbox).toHaveProperty('checked', false)
    fireEvent.click(checkbox)
    await waitFor(() => expect(checkbox).toHaveProperty('checked', true))
    expect(observerPanelAction).toHaveBeenLastCalledWith({ kind: 'automatic', enabled: true })
    fireEvent.click(screen.getByRole('button', { name: 'Forget paired browsers' }))
    expect(observerPanelAction).toHaveBeenCalledTimes(2)
    fireEvent.click(screen.getByRole('button', { name: 'Confirm forget' }))
    await waitFor(() => expect(observerPanelAction).toHaveBeenLastCalledWith({ kind: 'forget' }))
    await waitFor(() => expect(checkbox).toHaveProperty('checked', false))
  })
  it('isolates configuration shortcuts but preserves PTT release and the global Escape stop', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true })
    vi.mocked(observerPanelAction).mockResolvedValue({ running: false, address: '192.168.1.10', port: 7375, links: {} })
    const down = vi.fn()
    const up = vi.fn()
    const stop = vi.fn()
    function Scene() { useEscStop(true, stop); return <BrowserPanelShare panel="pota" /> }
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    try {
      render(<Scene />)
      const trigger = screen.getByRole('button', { name: 'Share this panel in a browser' })
      fireEvent.keyDown(trigger, { key: ' ', code: 'Space' })
      expect(down).not.toHaveBeenCalled()
      fireEvent.keyUp(trigger, { key: ' ', code: 'Space' })
      expect(up).toHaveBeenCalledTimes(1)
      fireEvent.click(trigger)
      const start = await screen.findByRole('button', { name: 'Start sharing' })
      fireEvent.keyDown(start, { key: ' ', code: 'Space' })
      fireEvent.keyDown(start, { key: 'Enter', code: 'Enter' })
      expect(fireEvent.keyDown(start, { key: 'F5', code: 'F5' })).toBe(false)
      expect(down).not.toHaveBeenCalled()
      fireEvent.keyUp(screen.getByLabelText('Laptop address'), { key: ' ', code: 'Space' })
      expect(up).toHaveBeenCalledTimes(2)
      fireEvent.keyDown(start, { key: 'Escape', code: 'Escape' })
      expect(stop).toHaveBeenCalledTimes(1)
      await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    } finally {
      window.removeEventListener('keydown', down)
      window.removeEventListener('keyup', up)
    }
  })

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
