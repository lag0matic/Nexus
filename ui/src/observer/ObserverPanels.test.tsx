// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ObserverPanels } from './ObserverPanels'
import { getNeedAlerts, getOtaSpots, getPropagation, getTvStation } from '../api'
import { listenToSpot, observerState, observerPanelData } from './client'

vi.mock('../api', () => ({ getNeedAlerts: vi.fn(), getOtaSpots: vi.fn(), getPropagation: vi.fn(), getTvStation: vi.fn() }))
vi.mock('./client', async (original) => ({ ...await original<typeof import('./client')>(), listenToSpot: vi.fn(), observerState: vi.fn(), observerPanelData: vi.fn() }))
vi.mock('../components/ConnectView', () => ({ ConnectView: ({ prop, onWorkSpot }: { prop: unknown; onWorkSpot: (spot: unknown) => void }) => <button disabled={!prop} onClick={() => onWorkSpot({call:'W9XYZ',freqMhz:14.06,mode:'CW'})}>Map spot</button> }))
beforeEach(() => {
  history.replaceState(null, '', '/panels/pota')
  vi.mocked(observerState).mockResolvedValue({ radio: 2, mhz: 14.074, band: '20m', mode: 'digital', tier: 'FT8', catOk: true, busy: false, observedAt: Date.now() })
  vi.mocked(listenToSpot).mockReset().mockResolvedValue(undefined)
  vi.mocked(getOtaSpots).mockResolvedValue([{ activator: 'W9XYZ', reference: 'US-1234', name: 'Test park', freqKhz: 14074, mode: 'FT8' }] as Awaited<ReturnType<typeof getOtaSpots>>)
  vi.mocked(getNeedAlerts).mockResolvedValue([{ call: 'W9XYZ', band: '20m', freqMhz: 14.08, exactMode: 'FT4', mode: 'Digital', entity: 'Test entity', headline: 'New band' }] as Awaited<ReturnType<typeof getNeedAlerts>>)
  vi.mocked(getPropagation).mockResolvedValue({} as Awaited<ReturnType<typeof getPropagation>>)
  vi.mocked(getTvStation).mockResolvedValue({call:'W9XYZ',grid:'EN37'})
})
afterEach(cleanup)

describe('browser observer panels', () => {
  it('automatically hides a logged activation on the next periodic panel refresh', async () => {
    vi.useFakeTimers()
    try {
      const spot = { activator: 'W9XYZ', reference: 'US-1234', name: 'Test park', freqKhz: 14288, mode: 'SSB', huntedToday: false }
      vi.mocked(getOtaSpots).mockResolvedValue([spot] as Awaited<ReturnType<typeof getOtaSpots>>)
      await act(async () => { render(<ObserverPanels />) })
      expect(screen.getByRole('button', { name: 'Listen to W9XYZ' })).toBeTruthy()
      vi.mocked(getOtaSpots).mockResolvedValue([{ ...spot, huntedToday: true }] as Awaited<ReturnType<typeof getOtaSpots>>)
      await act(async () => { await vi.advanceTimersByTimeAsync(30_000) })
      expect(screen.queryByRole('button', { name: 'Listen to W9XYZ' })).toBeNull()
      expect(screen.getByText('All matching spots worked today.')).toBeTruthy()
      expect(listenToSpot).not.toHaveBeenCalled()
    } finally {
      cleanup()
      vi.useRealTimers()
    }
  })

  it('hides only log-confirmed worked-today activations by default and can show them again', async () => {
    vi.mocked(getOtaSpots).mockResolvedValue([
      { activator: 'W9AAA', reference: 'US-0001', name: 'Worked park', freqKhz: 14060, mode: 'CW', huntedToday: true },
      { activator: 'W9AAA', reference: 'US-0002', name: 'Different park', freqKhz: 14250, mode: 'SSB', huntedToday: false },
      { activator: 'W9BBB', reference: 'US-0001', name: 'Another operator', freqKhz: 7074, mode: 'FT8', huntedToday: false },
      { activator: 'W9CCC', reference: 'US-0003', name: 'Legacy flag absent', freqKhz: 14260, mode: 'SSB' },
    ] as Awaited<ReturnType<typeof getOtaSpots>>)
    render(<ObserverPanels />)
    await screen.findByRole('button', { name: 'Listen to W9BBB' })
    expect(screen.getAllByRole('button', { name: 'Listen to W9AAA' })).toHaveLength(1)
    expect(screen.getByText(/1 worked today hidden/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Listen to W9CCC' })).toBeTruthy()
    fireEvent.click(screen.getByRole('checkbox', { name: 'Hide worked today' }))
    expect(screen.getAllByRole('button', { name: 'Listen to W9AAA' })).toHaveLength(2)
    expect(screen.getByText(/Worked today/)).toBeTruthy()
    expect(listenToSpot).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('checkbox', { name: 'Hide worked today' }))
    fireEvent.click(screen.getByRole('button', { name: '40m' }))
    expect(screen.queryByText(/worked today hidden/)).toBeNull()
    expect(screen.getAllByRole('button', { name: /^Listen to/ })).toHaveLength(1)
  })

  it('does not hide a tuned spot until the refreshed log flag confirms a contact, then restores it when eligible again', async () => {
    const spot = { activator: 'W9XYZ', reference: 'US-1234', name: 'Test park', freqKhz: 14288, mode: 'SSB', huntedToday: false }
    vi.mocked(getOtaSpots).mockResolvedValue([spot] as Awaited<ReturnType<typeof getOtaSpots>>)
    render(<ObserverPanels />)
    const row = await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('button', { name: 'Listen to W9XYZ' })).toBeTruthy()
    vi.mocked(getOtaSpots).mockResolvedValue([{ ...spot, huntedToday: true }] as Awaited<ReturnType<typeof getOtaSpots>>)
    fireEvent.click(screen.getByRole('button', { name: 'Refresh panel' }))
    await screen.findByText('All matching spots worked today.')
    expect(screen.queryByRole('button', { name: 'Listen to W9XYZ' })).toBeNull()
    vi.mocked(getOtaSpots).mockResolvedValue([spot] as Awaited<ReturnType<typeof getOtaSpots>>)
    fireEvent.click(screen.getByRole('button', { name: 'Refresh panel' }))
    await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    expect(listenToSpot).toHaveBeenCalledTimes(1)
  })

  it('carries a phone spot callsign and park to the laptop log draft', async () => {
    vi.mocked(getOtaSpots).mockResolvedValue([{ activator: 'W1AW/P', reference: 'US-1234', name: 'Test park', freqKhz: 14288, mode: 'SSB' }] as Awaited<ReturnType<typeof getOtaSpots>>)
    render(<ObserverPanels />)
    const row = await screen.findByRole('button', { name: 'Listen to W1AW/P' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.288, 'SSB', 2, {
      call: 'W1AW/P', park: { program: 'POTA', reference: 'US-1234' },
    }))
  })
  it('retries a failed panel read manually without pairing again', async () => {
    vi.mocked(getOtaSpots).mockRejectedValueOnce(new Error('Network unavailable'))
    render(<ObserverPanels />)
    await screen.findByRole('alert')
    fireEvent.click(screen.getByRole('button', { name: 'Refresh panel' }))
    await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    expect(screen.queryByRole('alert')).toBeNull()
    expect(listenToSpot).not.toHaveBeenCalled()
  })
  it('combines POTA band, exact mode and search filters without changing the radio', async () => {
    const base = { reference: 'US-1234', name: 'Park' }
    vi.mocked(getOtaSpots).mockResolvedValue([
      { ...base, activator: 'W9AAA', freqKhz: 14074, mode: 'FT8' },
      { ...base, activator: 'W9BBB', freqKhz: 7074, mode: 'FT8' },
      { ...base, activator: 'W9CCC', freqKhz: 14060, mode: 'CW' },
      { ...base, activator: 'W9DDD', freqKhz: 14250, mode: 'USB' },
    ] as Awaited<ReturnType<typeof getOtaSpots>>)
    render(<ObserverPanels />)
    await screen.findByRole('button', { name: 'Listen to W9AAA' })
    fireEvent.click(screen.getByRole('button', { name: '20m' }))
    fireEvent.click(screen.getByRole('button', { name: 'FT8' }))
    expect(screen.queryByRole('button', { name: 'Listen to W9BBB' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Listen to W9CCC' })).toBeNull()
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'nothing' } })
    expect(screen.queryByRole('button', { name: 'Listen to W9AAA' })).toBeNull()
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '' } })
    fireEvent.click(screen.getByRole('button', { name: 'SSB' }))
    expect(screen.getByRole('button', { name: 'Listen to W9DDD' })).toBeTruthy()
    expect(listenToSpot).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'All bands' }))
    fireEvent.click(screen.getByRole('button', { name: 'All modes' }))
    expect(screen.getAllByRole('button', { name: /^Listen to/ })).toHaveLength(4)
  })

  it('opens a monitor through the panel picker and only recalls its receive frequency', async () => {
    vi.mocked(observerPanelData).mockResolvedValue({ panel: 'memories', metrics: [], media: {}, groups: [
      { title: 'Receive memories', columns: ['Name', 'MHz', 'Mode'], rows: [['Test memory', '14.08', 'FT4']], total: 1 },
    ] })
    render(<ObserverPanels />)
    fireEvent.change(screen.getByRole('combobox', { name: 'Panel' }), { target: { value: 'memories' } })
    const row = await screen.findByRole('button', { name: 'Listen to Test memory' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    expect(observerPanelData).toHaveBeenCalledWith('memories')
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.08, 'FT4', 2))
    expect(location.pathname).toBe('/panels/memories')
  })

  it('tunes a POTA row with its exact mode, without any work/QSO action', async () => {
    render(<ObserverPanels />)
    const row = await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.074, 'FT8', 2, { call: 'W9XYZ', park: { program: 'POTA', reference: 'US-1234' } }))
  })

  it('navigates to Needed and keeps FT4 rather than guessing FT8', async () => {
    render(<ObserverPanels />)
    fireEvent.click(screen.getByRole('link', { name: 'Needed' }))
    const row = await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.08, 'FT4', 2, { call: 'W9XYZ', park: null }))
    expect(location.pathname).toBe('/panels/needed')
  })

  it('keeps rows readable but disables tuning while the radio is transmitting', async () => {
    vi.mocked(observerState).mockResolvedValue({ radio: 2, mhz: 14.074, band: '20m', mode: 'digital', tier: 'FT8', catOk: true, busy: true, observedAt: Date.now() })
    render(<ObserverPanels />)
    const row = await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    expect((row as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(row)
    expect(listenToSpot).not.toHaveBeenCalled()
  })

  it('uses the same receive-only action for a Conditions map spot', async () => {
    render(<ObserverPanels />)
    fireEvent.click(screen.getByRole('link', { name: 'Conditions' }))
    const spot = await screen.findByRole('button', { name: 'Map spot' })
    await waitFor(() => expect((spot as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(spot)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.06, 'CW', 2, { call: 'W9XYZ' }))
  })
})
