// The browser has two transports: allowlisted GET panel data and this sole POST.
// No Tauri bridge, transmit command, arming flag, retry or operation queue.
export type ObserverRadio = {
  radio: number
  mhz: number
  band: string
  mode: string
  tier: string
  catOk: boolean | null
  busy: boolean
  observedAt: number
}

export async function observerState(): Promise<ObserverRadio> {
  return request('/observer/read/state')
}

export type MonitorData = {
  panel: string
  metrics: [string, string][]
  groups: { title: string; columns: string[]; rows: string[][]; total: number }[]
  media: { spectrum?: number[]; loHz?: number; hiHz?: number; rgb?: string; width?: number; height?: number; bmp?: string; stations?: import('../types').Station[]; myGrid?: string }
}
export function observerPanelData(panel: import('./panels').ObserverPanel): Promise<MonitorData> {
  return request(`/observer/read/observer_panel_data?args=${encodeURIComponent(JSON.stringify({ panel }))}`)
}

const ACCESS_KEY = 'nexus.browser-panels.access'

export function restoreBrowserAccess(): boolean {
  if (window.__NEXUS_OBSERVER_ACCESS__) return true
  try {
    const access = localStorage.getItem(ACCESS_KEY) ?? sessionStorage.getItem(ACCESS_KEY)
    if (access && /^[a-f0-9]{64}$/.test(access)) window.__NEXUS_OBSERVER_ACCESS__ = access
  } catch { /* Storage-denied browsers can still pair for this page. */ }
  return Boolean(window.__NEXUS_OBSERVER_ACCESS__)
}

export function forgetBrowserAccess() {
  delete window.__NEXUS_OBSERVER_ACCESS__
  try { localStorage.removeItem(ACCESS_KEY); sessionStorage.removeItem(ACCESS_KEY) } catch { /* Page memory is cleared regardless. */ }
  window.dispatchEvent(new Event('observer-access-lost'))
}

export async function pairBrowser(entered: string, remember = true): Promise<void> {
  const code = entered.replace(/\s/g, '')
  if (!/^\d{8}$/.test(code)) throw new Error('Enter the eight-digit code shown in Nexus')
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), 3000)
  try {
    const response = await fetch('/observer/pair', { method: 'POST', signal: controller.signal,
      cache: 'no-store', credentials: 'omit', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code }) })
    if (!response.ok) throw new Error(await response.text())
    const result = await response.json() as { access?: unknown; persistent?: boolean }
    if (typeof result.access !== 'string' || !/^[a-f0-9]{64}$/.test(result.access)) throw new Error('Invalid access response')
    window.__NEXUS_OBSERVER_ACCESS__ = result.access
    try {
      localStorage.removeItem(ACCESS_KEY); sessionStorage.removeItem(ACCESS_KEY)
      // Session storage survives refresh. Durable storage is only useful when
      // Nexus confirms its own pairing was saved in the OS credential store.
      const storage = remember && result.persistent === true ? localStorage : sessionStorage
      storage.setItem(ACCESS_KEY, result.access)
    } catch { /* Pairing still works when browser storage is denied. */ }
  } finally { window.clearTimeout(timeout) }
}

async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const access = window.__NEXUS_OBSERVER_ACCESS__
  if (!access) throw new Error('Enter the access code shown in Nexus')
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), 3000)
  try {
    const result = await fetch(url, { ...init, signal: controller.signal, cache: 'no-store', credentials: 'omit',
      headers: { Authorization: `Bearer ${access}`, ...(init?.headers ?? {}) } })
    if (!result.ok) {
      const message = await result.text()
      if (result.status === 403 && message === 'Enter the current access code shown in Nexus' && window.__NEXUS_OBSERVER_ACCESS__ === access) forgetBrowserAccess()
      throw new Error(message)
    }
    return await result.json() as T
  } finally { window.clearTimeout(timeout) }
}

export async function listenToSpot(mhz: number, mode: string, radio: number): Promise<void> {
  // Refresh immediately before the deliberate click. A changed active radio is
  // not silently adopted. Only the server's clock stamps request freshness.
  const pressed = performance.now()
  const state = await observerState()
  if (performance.now() - pressed > 2500) throw new Error('The click expired; try again')
  if (state.radio !== radio) throw new Error('The active radio changed; click again after refreshing')
  if (state.catOk !== true || state.busy) throw new Error('The radio is busy or disconnected')
  await request('/observer/listen', { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ radio, mhz, mode, observedAt: state.observedAt }) })
}

export const LISTEN_MODES = new Set(['CW', 'SSB', 'USB', 'LSB', 'AM', 'FM', 'RTTY', 'PSK31', 'BPSK31', 'FT8', 'FT4', 'FT2', 'JS8', 'JS8CALL', 'WSPR', 'FST4', 'FST4W', 'Q65', 'JT65', 'MSK144'])

export function listenMode(exact: string | null | undefined, category?: string): string | null {
  const mode = exact?.trim().toUpperCase()
  if (mode && LISTEN_MODES.has(mode)) return mode
  if (category === 'CW') return 'CW'
  if (category === 'Phone') return 'SSB'
  return null
}
