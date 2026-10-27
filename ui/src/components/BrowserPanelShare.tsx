import { useId, useRef, useState } from 'react'
import { Check, Copy, MonitorSmartphone, Power, X } from 'lucide-react'
import { observerPanelAction, type ObserverPanelStatus } from '../api'
import { Dialog } from './ui/Dialog'
import './browser-panel-share.css'

export function BrowserPanelShare({ panel }: { panel: 'pota' | 'conditions' | 'needed' }) {
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<ObserverPanelStatus | null>(null)
  const [address, setAddress] = useState('')
  const [port, setPort] = useState(7375)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)
  const flight = useRef(false)
  const opener = useRef<HTMLButtonElement>(null)
  const id = useId()
  const native = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (!native) return null

  const refresh = async () => {
    if (flight.current) return
    flight.current = true; setBusy(true); setError('')
    try {
      const next = await observerPanelAction({ kind: 'status' })
      setStatus(next); setAddress(next.address); setPort(next.port)
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { flight.current = false; setBusy(false) }
  }
  const toggle = async () => {
    if (flight.current || !status) return
    flight.current = true; setBusy(true); setError(''); setCopied(false)
    try {
      const next = await observerPanelAction(status.running ? { kind: 'stop' } : { kind: 'start', address, port })
      setStatus(next); setAddress(next.address); setPort(next.port)
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { flight.current = false; setBusy(false) }
  }
  const link = status?.links[panel]
  return <>
    <button ref={opener} type="button" className="browser-share-trigger" title="Share this panel in a browser" aria-label="Share this panel in a browser"
      onClick={() => { setOpen(true); setCopied(false); void refresh() }}><MonitorSmartphone size={17} aria-hidden="true" /></button>
    <Dialog open={open} onOpenChange={setOpen} title="Browser panels" description="Private LAN · HTTP"
      onCloseAutoFocus={(e) => { e.preventDefault(); opener.current?.focus({ preventScroll: true }) }}>
      <div className="browser-share-fields">
        <label htmlFor={`${id}-ip`}>Laptop address<input id={`${id}-ip`} value={address} disabled={busy || status?.running}
          onChange={(e) => setAddress(e.target.value)} spellCheck={false} /></label>
        <label htmlFor={`${id}-port`}>Port<input id={`${id}-port`} type="number" min={1024} max={65535} value={port} disabled={busy || status?.running}
          onChange={(e) => setPort(Number(e.target.value))} /></label>
      </div>
      <p className="browser-share-warning">Unencrypted access. Trusted LAN only.</p>
      {error && <p role="alert">{error}</p>}
      {link && status?.running && <label className="browser-share-link" htmlFor={`${id}-link`}>Access link
        <span><input id={`${id}-link`} value={link} readOnly onFocus={(e) => e.target.select()} />
          <button type="button" title={copied ? 'Copied' : 'Copy access link'} aria-label="Copy access link" onClick={() => {
            if (!navigator.clipboard?.writeText) { setError('Clipboard unavailable; select the access link'); return }
            void navigator.clipboard.writeText(link).then(() => setCopied(true)).catch(() => setError('Clipboard unavailable; select the access link'))
          }}>{copied ? <Check size={17} /> : <Copy size={17} />}</button></span>
      </label>}
      <div className="browser-share-actions">
        <button type="button" disabled={busy || !status || !address || !Number.isInteger(port) || port < 1024 || port > 65535}
          onClick={() => void toggle()}><Power size={16} aria-hidden="true" /> {status?.running ? 'Stop sharing' : 'Start sharing'}</button>
        <button type="button" onClick={() => setOpen(false)}><X size={16} aria-hidden="true" /> Close</button>
      </div>
    </Dialog>
  </>
}
