// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ObserverPanels } from './ObserverPanels'
import { getNeedAlerts, getOtaSpots, getPropagation, getTvStation } from '../api'
import { listenToSpot, observerState } from './client'

vi.mock('../api', () => ({ getNeedAlerts: vi.fn(), getOtaSpots: vi.fn(), getPropagation: vi.fn(), getTvStation: vi.fn() }))
vi.mock('./client', async (original) => ({ ...await original<typeof import('./client')>(), listenToSpot: vi.fn(), observerState: vi.fn() }))
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
