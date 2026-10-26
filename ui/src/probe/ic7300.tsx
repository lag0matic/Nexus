import '../styles.css'
import { createRoot } from 'react-dom/client'
import { PhoneCockpit } from '../components/PhoneCockpit'
import { useViewport } from '../useViewport'
import settings from '../components/__fixtures__/defaultSettings.json'
import snapshot from '../../../crates/tempo-app/tests/fixtures/station_identity_snapshot.json'
import type { AppSnapshot } from '../types'
import { emptyAnswer, type LogQuestion } from '../features/logAnswers'

document.documentElement.dataset.theme = 'dark'
window.__TAURI_INTERNALS__ = {
  invoke: async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === 'ask_log') return emptyAnswer(args?.q as LogQuestion) as T
    if (command === 'set_notch_position') {
      document.documentElement.dataset.notchCommand = JSON.stringify(args)
      return {} as T
    }
    if (command === 'get_settings') return settings as T
    if (command === 'get_audio_devices') return { input: [], output: [] } as T
    return [] as T
  },
}
const snap = {
  ...snapshot,
  mycall: 'N0CALL', mygrid: 'EN52',
  radio: { ...snapshot.radio, dialMhz: 7.2, band: '40m', sideband: 'LSB', rigMode: 'LSB', catOk: true,
    transmitting: false, txEnabled: false, txAllowed: true, slot: 0, nextSlotMs: 0,
    notchPositionRaw: 127, manualNotch: true, notch: false },
  link: {}, qso: {}, stations: [], conversations: [],
} as unknown as AppSnapshot
function Probe() {
  useViewport(1)
  return <div className="app"><PhoneCockpit snap={snap} theme="dark" /></div>
}
createRoot(document.getElementById('root')!).render(<Probe />)
