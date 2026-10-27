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

export async function pairBrowser(entered: string): Promise<void> {
  const code = entered.replace(/\s/g, '')
  if (!/^\d{8}$/.test(code)) throw new Error('Enter the eight-digit code shown in Nexus')
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), 3000)
  try {
    const response = await fetch('/observer/pair', { method: 'POST', signal: controller.signal,
      cache: 'no-store', credentials: 'omit', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code }) })
    if (!response.ok) throw new Error(await response.text())
    const result = await response.json() as { access?: unknown }
    if (typeof result.access !== 'string' || !/^[a-f0-9]{64}$/.test(result.access)) throw new Error('Invalid access response')
    window.__NEXUS_OBSERVER_ACCESS__ = result.access
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
    if (!result.ok) throw new Error(await result.text())
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
