// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest'
import { listenMode, listenToSpot, observerPanelData, observerState, pairBrowser, restoreBrowserAccess, forgetBrowserAccess } from './client'

const state = { radio: 2, catOk: true, busy: false, observedAt: 1234 }
afterEach(() => { vi.unstubAllGlobals(); vi.restoreAllMocks(); localStorage.clear(); sessionStorage.clear(); delete window.__NEXUS_OBSERVER_ACCESS__ })

describe('receive-only browser transport', () => {
  it('carries only optional callsign and park draft hints alongside the receive selection', async () => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    const fetcher = vi.fn().mockResolvedValueOnce(Response.json(state)).mockResolvedValueOnce(Response.json({ accepted: true }))
    vi.stubGlobal('fetch', fetcher)
    await listenToSpot(14.288, 'SSB', 2, { call: 'W1AW/P', park: { program: 'POTA', reference: 'US-1234' } })
    expect(fetcher.mock.calls.map(c => c[0])).toEqual(['/observer/read/state', '/observer/listen'])
    expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ radio: 2, mhz: 14.288, mode: 'SSB', observedAt: 1234,
      call: 'W1AW/P', park: { program: 'POTA', reference: 'US-1234' } })
  })
  it('clears revoked access but never discards pairing for an offline server or ordinary panel error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access: 'a'.repeat(64), persistent: true })))
    await pairBrowser('12345678')
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')))
    await expect(observerState()).rejects.toThrow('offline')
    expect(localStorage.length).toBe(1)
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Operation not allowed', { status: 403 })))
    await expect(observerPanelData('memories')).rejects.toThrow()
    expect(localStorage.length).toBe(1)
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('Enter the current access code shown in Nexus', { status: 403 })))
    await expect(observerState()).rejects.toThrow()
    expect(localStorage.length).toBe(0)
    expect(restoreBrowserAccess()).toBe(false)
  })
  it('forgets this browser without issuing any radio or server operation', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access: 'a'.repeat(64), persistent: true })))
    await pairBrowser('12345678')
    const fetcher = vi.fn()
    vi.stubGlobal('fetch', fetcher)
    forgetBrowserAccess()
    expect(restoreBrowserAccess()).toBe(false)
    expect(fetcher).not.toHaveBeenCalled()
  })
  it('falls back to session storage when Nexus could not save its own pairing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json({ access: 'a'.repeat(64), persistent: false })))
    await pairBrowser('12345678')
    expect(localStorage.length).toBe(0)
    expect(sessionStorage.length).toBe(1)
  })
  it('fetches monitor data by authenticated GET, never through the command bridge', async () => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    const fetcher = vi.fn().mockResolvedValue(Response.json({ panel: 'memories' }))
    vi.stubGlobal('fetch', fetcher)
    await observerPanelData('memories')
    const [url, options] = fetcher.mock.calls[0]
    expect(new URL(url, 'http://laptop').pathname).toBe('/observer/read/observer_panel_data')
    expect(JSON.parse(new URL(url, 'http://laptop').searchParams.get('args')!)).toEqual({ panel: 'memories' })
    expect(options.method).toBeUndefined()
    expect(options.headers.Authorization).toBe('Bearer test-only-access')
    expect(fetcher).toHaveBeenCalledTimes(1)
  })
  it('refreshes state and sends only frequency, exact mode and radio to the sole listen endpoint', async () => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    const fetcher = vi.fn().mockResolvedValueOnce(Response.json(state)).mockResolvedValueOnce(Response.json({ accepted: true }))
    vi.stubGlobal('fetch', fetcher)
    await listenToSpot(14.074, 'FT8', 2)
    expect(fetcher.mock.calls.map((c) => c[0])).toEqual(['/observer/read/state', '/observer/listen'])
    const post = fetcher.mock.calls[1][1]
    expect(post.method).toBe('POST')
    expect(JSON.parse(post.body)).toEqual({ radio: 2, mhz: 14.074, mode: 'FT8', observedAt: 1234 })
    expect(post.headers.Authorization).toBe('Bearer test-only-access')
  })

  it.each([{ busy: true }, { catOk: false }, { radio: 3 }])('never queues a click when state changes: %o', async (change) => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    const fetcher = vi.fn().mockResolvedValue(Response.json({ ...state, ...change }))
    vi.stubGlobal('fetch', fetcher)
    await expect(listenToSpot(14.074, 'FT8', 2)).rejects.toThrow()
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('does not retry a failed or uncertain tuning request', async () => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    const fetcher = vi.fn().mockResolvedValueOnce(Response.json(state)).mockRejectedValueOnce(new Error('connection lost'))
    vi.stubGlobal('fetch', fetcher)
    await expect(listenToSpot(14.074, 'FT8', 2)).rejects.toThrow('connection lost')
    expect(fetcher).toHaveBeenCalledTimes(2)
  })

  it('expires a click if refreshing state took too long, even if the reply itself is fresh', async () => {
    window.__NEXUS_OBSERVER_ACCESS__ = 'test-only-access'
    vi.spyOn(performance, 'now').mockReturnValueOnce(0).mockReturnValueOnce(3001)
    const fetcher = vi.fn().mockResolvedValue(Response.json(state))
    vi.stubGlobal('fetch', fetcher)
    await expect(listenToSpot(14.074, 'FT8', 2)).rejects.toThrow('expired')
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('does not guess FT8 from an unspecified Digital label', () => {
    expect(listenMode(null, 'Digital')).toBeNull()
    expect(listenMode('FT4', 'Digital')).toBe('FT4')
    expect(listenMode(null, 'CW')).toBe('CW')
    expect(listenMode('ALIEN', 'Digital')).toBeNull()
  })
})
