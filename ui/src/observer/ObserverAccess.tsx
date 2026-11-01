import { useEffect, useRef, useState } from 'react'
import { LogIn } from 'lucide-react'
import { ObserverPanels } from './ObserverPanels'
import { pairBrowser, restoreBrowserAccess } from './client'

export function ObserverAccess() {
  const [connected, setConnected] = useState(restoreBrowserAccess)
  const [remember, setRemember] = useState(true)
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const flight = useRef(false)
  useEffect(() => {
    const lost = () => { setConnected(false); setError('Access expired. Enter the current code from Nexus.') }
    window.addEventListener('observer-access-lost', lost)
    return () => window.removeEventListener('observer-access-lost', lost)
  }, [])
  if (connected) return <ObserverPanels />
  return <main className="app observer-access">
    <form onSubmit={(e) => {
      e.preventDefault()
      if (flight.current) return
      flight.current = true; setBusy(true); setError('')
      void pairBrowser(code, remember).then(() => { setCode(''); setConnected(true) })
        .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
        .finally(() => { flight.current = false; setBusy(false) })
    }}>
      <h1>Nexus</h1>
      <label htmlFor="observer-code">Access code</label>
      <input id="observer-code" type="text" inputMode="numeric" autoComplete="off" spellCheck={false}
        maxLength={9} value={code} disabled={busy} onChange={(e) => setCode(e.target.value)} />
      <label><input type="checkbox" checked={remember} disabled={busy} onChange={e => setRemember(e.target.checked)} /> Remember this browser</label>
      {error && <p role="alert">{error}</p>}
      <button type="submit" disabled={busy || !/^\d{8}$/.test(code.replace(/\s/g, ''))}><LogIn size={16} aria-hidden="true" />Connect</button>
    </form>
  </main>
}
