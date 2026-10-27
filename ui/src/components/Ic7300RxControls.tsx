import { useEffect, useRef, useState } from 'react'
import { RotateCcw } from 'lucide-react'
import { setIc7300RxControl } from '../api'
import { pushToast } from '../toast'
import type { RadioStatus } from '../types'
import { WheelRange } from './WheelRange'

export function Ic7300RxControls({ radio }: { radio: RadioStatus }) {
  const reported = radio.ic7300RxControls ?? [null, null, null]
  const [positions, setPositions] = useState([reported[0] ?? 128, reported[1] ?? 128])
  const dragging = useRef([false, false])
  useEffect(() => {
    setPositions(old => old.map((v, i) => dragging.current[i] ? v : reported[i] ?? 128))
  }, [radio]) // eslint-disable-line react-hooks/exhaustive-deps
  const change = (control: 0 | 1 | 2, value: number) => {
    if (control < 2) setPositions(old => old.map((v, i) => i === control ? value : v))
    return setIc7300RxControl(control, value)
      .catch(e => pushToast(e instanceof Error ? e.message : String(e), 'error'))
  }
  const disabled = radio.catOk !== true
  return <>
    {([0, 1] as const).map(control => reported[control] != null &&
      <label key={control} className="ph-dsplev" title="Radio PBT knob position; 0 is centered. This scale is not Hz.">
        <span>{control === 0 ? 'PBT inner' : 'PBT outer'}</span>
        <WheelRange wheelStep={1} type="range" min={0} max={255} disabled={disabled}
          value={positions[control]} aria-label={control === 0 ? 'PBT inner' : 'PBT outer'}
          onChange={e => void change(control, Number(e.target.value))}
          onPointerDown={() => { dragging.current[control] = true }}
          onPointerUp={() => { dragging.current[control] = false }}
          onPointerCancel={() => { dragging.current[control] = false }}
          onBlur={() => { dragging.current[control] = false }} />
        <span className="ph-power-val">{positions[control] - 128}</span>
      </label>)}
    {reported[0] != null && reported[1] != null &&
      <button type="button" className="theme-chip" aria-label="Center both PBT controls"
        title="Center both PBT controls" disabled={disabled}
        onClick={() => { void change(0, 128); void change(1, 128) }}><RotateCcw size={16} /></button>}
    {reported[2] != null && <label className="ph-dsplev">
      <span>Notch width</span>
      <select className="settings-input" aria-label="Notch width" value={reported[2]} disabled={disabled}
        onChange={e => void change(2, Number(e.target.value))}>
        <option value={0}>Wide</option><option value={1}>Mid</option><option value={2}>Narrow</option>
      </select>
    </label>}
  </>
}
