import { useRef, useState } from 'react'
import { setTuner } from '../api'
import { pushToast } from '../toast'
import { isOnAir, type RadioStatus } from '../types'

/** In-line/bypass only. Starting a matching cycle remains the separate ATU action. */
export function TunerToggle({ radio, onCarrier, carrierDisabled = !radio.txAllowed, carrierTitle }: {
  radio: RadioStatus
  onCarrier?: (on: boolean) => void
  carrierDisabled?: boolean
  carrierTitle?: string
}) {
  const [busy, setBusy] = useState(false)
  const flight = useRef(false)
  const stopping = radio.tuning && !!onCarrier
  const unavailable = radio.catOk !== true || radio.atu == null || !radio.tunerSwitchSupported
  const disabled = !stopping && (busy || unavailable || radio.tuning || isOnAir(radio))
  const toggle = async () => {
    if (stopping) { onCarrier?.(false); return }
    if (disabled || flight.current) return
    flight.current = true
    setBusy(true)
    try { await setTuner(!radio.atu) }
    catch (e) { pushToast(e instanceof Error ? e.message : String(e), 'error') }
    finally { flight.current = false; setBusy(false) }
  }
  // Other radio models retain their original carrier control. The IC-7300 model flag is
  // present before tuner discovery, so a reconnect never temporarily offers a carrier.
  if (!radio.tunerSwitchSupported) return <button type="button"
    className={`op-btn tune${radio.tuning ? ' keyed' : ''}`} aria-pressed={radio.tuning}
    onClick={() => onCarrier?.(!radio.tuning)} disabled={!stopping && carrierDisabled}
    title={carrierTitle ?? radio.txRefusal ?? 'Key a steady carrier to tune an ATU/amp (auto-stops on the tune watchdog). Click again to stop.'}>Tune</button>
  return <button type="button" className={`op-btn tune${radio.atu ? ' on' : ''}${stopping ? ' keyed' : ''}`}
    aria-pressed={stopping || radio.atu === true} aria-busy={busy} disabled={disabled}
    onClick={() => void toggle()}
    title={stopping ? 'Stop the tune carrier' : unavailable ? 'This connection has not reported a supported IC-7300 tuner'
      : isOnAir(radio) || radio.txBusyReason ? 'Stop transmitting before switching the tuner'
        : radio.atu ? 'Internal tuner is in line. Click to bypass. ATU starts a tuning cycle.'
          : 'Internal tuner is bypassed. Click to switch it in. ATU starts a tuning cycle.'}>
    Tune
  </button>
}
