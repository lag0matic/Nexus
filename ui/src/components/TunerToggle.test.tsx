// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TunerToggle } from './TunerToggle'
import type { RadioStatus } from '../types'
const api = vi.hoisted(() => ({ setTuner: vi.fn(async () => ({})) }))
vi.mock('../api', () => api)
vi.mock('../toast', () => ({ pushToast: vi.fn() }))
afterEach(() => { cleanup(); vi.clearAllMocks() })
const radio = (patch = {}) => ({ catOk: true, atu: false, tunerSwitchSupported: true, txAllowed: true, ...patch }) as RadioStatus
describe('Tune switches the internal tuner without a carrier', () => {
  it('switches in and bypasses using the reported state, never the carrier callback', async () => {
    const carrier = vi.fn()
    const view = render(<TunerToggle radio={radio()} onCarrier={carrier} />)
    fireEvent.click(screen.getByRole('button', { name: 'Tune' }))
    await waitFor(() => expect(api.setTuner).toHaveBeenCalledWith(true))
    await waitFor(() => expect(screen.getByRole('button').getAttribute('aria-busy')).toBe('false'))
    view.rerender(<TunerToggle radio={radio({ atu: true })} onCarrier={carrier} />)
    fireEvent.click(screen.getByRole('button'))
    await waitFor(() => expect(api.setTuner).toHaveBeenLastCalledWith(false))
    expect(carrier).not.toHaveBeenCalled()
  })
  it.each([{ atu: null }, { catOk: false }, { rigKeyed: true }, { transmitting: true }, { txBusyReason: 'ATU' }])(
    'refuses unavailable or keyed state %j', patch => {
      render(<TunerToggle radio={radio(patch)} />)
      expect((screen.getByRole('button') as HTMLButtonElement).disabled).toBe(true)
      fireEvent.click(screen.getByRole('button'))
      expect(api.setTuner).not.toHaveBeenCalled()
    })
  it('can stop an already-running carrier even without tuner capability', () => {
    const stop = vi.fn()
    render(<TunerToggle radio={radio({ tuning: true, atu: null })} onCarrier={stop} />)
    fireEvent.click(screen.getByRole('button'))
    expect(stop).toHaveBeenCalledWith(false)
    expect(api.setTuner).not.toHaveBeenCalled()
  })
  it('can bypass or insert a tuner with TX disabled because this action never transmits', async () => {
    render(<TunerToggle radio={radio({ txEnabled: false, txAllowed: false })} />)
    fireEvent.click(screen.getByRole('button'))
    await waitFor(() => expect(api.setTuner).toHaveBeenCalledWith(true))
  })
  it('preserves the existing carrier action on other radio models', () => {
    const carrier = vi.fn()
    render(<TunerToggle radio={radio({ tunerSwitchSupported: false })} onCarrier={carrier} />)
    fireEvent.click(screen.getByRole('button'))
    expect(carrier).toHaveBeenCalledWith(true)
    expect(api.setTuner).not.toHaveBeenCalled()
  })
})
