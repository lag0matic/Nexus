import { useId, useRef, useState, type KeyboardEvent } from 'react'
import { Check, Copy, MonitorSmartphone, Power, X, Unplug } from 'lucide-react'
import { observerPanelAction, type ObserverPanelStatus } from '../api'
import { Dialog } from './ui/Dialog'
import './browser-panel-share.css'
import type { ObserverPanel } from '../observer/panels'

function sharingKeys(event: KeyboardEvent<HTMLElement>) {
  if (event.key === 'Escape' || event.key === 'Tab') return
  // Configuration keys are not PTT/macros. Never swallow keyup: a held Space
  // must still release PTT if focus moved here. Escape retains the global stop.
  if (/^F\d{1,2}$/.test(event.code) && !event.altKey && !event.ctrlKey && !event.metaKey) event.preventDefault()
  event.stopPropagation()
}

export function BrowserPanelShare({ panel, label = 'Share this panel in a browser' }: { panel: ObserverPanel; label?: string }) {
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<ObserverPanelStatus | null>(null)
  const [address, setAddress] = useState('')
  const [port, setPort] = useState(7375)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)
  const [confirmForget, setConfirmForget] = useState(false)
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
  const configure = async (action: { kind: 'automatic'; enabled: boolean } | { kind: 'forget' }) => {
    if (flight.current) return
    flight.current = true; setBusy(true); setError('')
    try {
      const next = await observerPanelAction(action)
      setStatus(next); setAddress(next.address); setPort(next.port)
    } catch (e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { flight.current = false; setBusy(false) }
  }
  return <>
    <button ref={opener} type="button" className="browser-share-trigger" title={label} aria-label={label}
      onKeyDown={sharingKeys}
      onClick={() => { setOpen(true); setCopied(false); setConfirmForget(false); void refresh() }}><MonitorSmartphone size={17} aria-hidden="true" /></button>
    <Dialog open={open} onOpenChange={setOpen} title="Browser panels" description="Private LAN · HTTP"
      onCloseAutoFocus={(e) => { e.preventDefault(); opener.current?.focus({ preventScroll: true }) }}>
      <div onKeyDown={sharingKeys}>
      <div className="browser-share-fields">
        <label htmlFor={`${id}-ip`}>Laptop address<input id={`${id}-ip`} value={address} disabled={busy || status?.running}
          onChange={(e) => setAddress(e.target.value)} spellCheck={false} /></label>
        <label htmlFor={`${id}-port`}>Port<input id={`${id}-port`} type="number" min={1024} max={65535} value={port} disabled={busy || status?.running}
          onChange={(e) => setPort(Number(e.target.value))} /></label>
      </div>
      <p className="browser-share-warning">Unencrypted access. Trusted LAN only.</p>
      <label className="browser-share-startup"><input type="checkbox" checked={status?.automatic ?? false}
        disabled={busy || !status?.persistent} onChange={e => void configure({ kind: 'automatic', enabled: e.target.checked })} /> Start browser sharing with Nexus</label>
      {status?.storageError && <p role="status" className="browser-share-warning">{status.storageError}</p>}
      {error && <p role="alert">{error}</p>}
      {link && status?.running && <><label className="browser-share-link" htmlFor={`${id}-link`}>Browser address
        <span><input id={`${id}-link`} value={link} readOnly onFocus={(e) => e.target.select()} />
          <button type="button" title={copied ? 'Copied' : 'Copy access link'} aria-label="Copy access link" onClick={() => {
            if (!navigator.clipboard?.writeText) { setError('Clipboard unavailable; select the access link'); return }
            void navigator.clipboard.writeText(link).then(() => setCopied(true)).catch(() => setError('Clipboard unavailable; select the access link'))
          }}>{copied ? <Check size={17} /> : <Copy size={17} />}</button></span>
      </label><label className="browser-share-link" htmlFor={`${id}-code`}>Access code
        <input id={`${id}-code`} value={status.accessCode?.replace(/(.{4})(.{4})/, '$1 $2') ?? ''} readOnly onFocus={(e) => e.target.select()} />
      </label></>}
      <div className="browser-share-actions">
        {confirmForget && <p role="alert">Forget all paired browsers, disable automatic startup, and stop sharing?</p>}
        {confirmForget && <button type="button" onClick={() => setConfirmForget(false)}>Cancel forget</button>}
        <button type="button" disabled={busy || !status || !status.persistent && !status.running}
          onClick={() => { if (confirmForget) { setConfirmForget(false); void configure({ kind: 'forget' }) } else setConfirmForget(true) }}><Unplug size={16} aria-hidden="true" /> {confirmForget ? 'Confirm forget' : 'Forget paired browsers'}</button>
        <button type="button" data-sharing-toggle disabled={busy || !status || !address || !Number.isInteger(port) || port < 1024 || port > 65535}
          onClick={() => void toggle()}><Power size={16} aria-hidden="true" /> {status?.running ? 'Stop sharing' : 'Start sharing'}</button>
        <button type="button" onClick={() => setOpen(false)}><X size={16} aria-hidden="true" /> Close</button>
      </div>
      </div>
    </Dialog>
  </>
}
