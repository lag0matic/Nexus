// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.074, 'FT8', 2))
  })

  it('navigates to Needed and keeps FT4 rather than guessing FT8', async () => {
    render(<ObserverPanels />)
    fireEvent.click(screen.getByRole('link', { name: 'Needed' }))
    const row = await screen.findByRole('button', { name: 'Listen to W9XYZ' })
    await waitFor(() => expect((row as HTMLButtonElement).disabled).toBe(false))
    fireEvent.click(row)
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.08, 'FT4', 2))
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
    await waitFor(() => expect(listenToSpot).toHaveBeenCalledWith(14.06, 'CW', 2))
  })
})
