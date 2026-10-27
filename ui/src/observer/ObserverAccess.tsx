import { useRef, useState } from 'react'
import { LogIn } from 'lucide-react'
import { ObserverPanels } from './ObserverPanels'
import { pairBrowser } from './client'

export function ObserverAccess() {
  const [connected, setConnected] = useState(Boolean(window.__NEXUS_OBSERVER_ACCESS__))
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const flight = useRef(false)
  if (connected) return <ObserverPanels />
  return <main className="app observer-access">
    <form onSubmit={(e) => {
      e.preventDefault()
      if (flight.current) return
      flight.current = true; setBusy(true); setError('')
      void pairBrowser(code).then(() => { setCode(''); setConnected(true) })
        .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
        .finally(() => { flight.current = false; setBusy(false) })
    }}>
      <h1>Nexus</h1>
      <label htmlFor="observer-code">Access code</label>
      <input id="observer-code" type="text" inputMode="numeric" autoComplete="off" spellCheck={false}
        maxLength={9} value={code} disabled={busy} onChange={(e) => setCode(e.target.value)} />
      {error && <p role="alert">{error}</p>}
      <button type="submit" disabled={busy || !/^\d{8}$/.test(code.replace(/\s/g, ''))}><LogIn size={16} aria-hidden="true" />Connect</button>
    </form>
  </main>
}
