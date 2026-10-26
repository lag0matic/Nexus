// Browser-only fixture. This is not a production entry point and makes no network/API writes.
import '../styles.css'
import { createRoot } from 'react-dom/client'
import { SettingsPanel } from '../components/SettingsPanel'
import { useViewport } from '../useViewport'
import type { FeaturesApi } from '../useFeatures'
import settings from '../components/__fixtures__/defaultSettings.json'

document.documentElement.dataset.theme = 'dark'
let stationSettings = { ...settings, mycall: 'N0CALL', longwaveUrl: 'https://log.example.invalid', longwaveLogbookId: '', longwaveUpload: false }
window.__TAURI_INTERNALS__ = {
  invoke: async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === 'set_settings') {
      stationSettings = { ...stationSettings, ...args?.settings as object }
      document.documentElement.dataset.longwaveSaved = JSON.stringify({
        book: stationSettings.longwaveLogbookId, upload: stationSettings.longwaveUpload,
      })
      return {} as T
    }
    const answers: Record<string, unknown> = {
      get_settings: stationSettings,
      get_rig_models: [], get_all_rig_models: [], get_serial_ports_detailed: [], get_band_plan: [],
      get_audio_devices: { input: [], output: [] }, get_credentials_status: [], get_connection_log: [],
      detect_rigs: [], app_version: '1.19.0',
      longwave_logbooks: [{ id: 'book-a', name: 'Home', operator_callsign: 'N0CALL', contact_count: 0 }],
    }
    if (!(command in answers)) throw new Error(`Unanswered probe command: ${command}`)
    return answers[command] as T
  },
}
const features = {
  enabled: () => true, setEnabled: () => {}, all: () => [], profile: 'full', setProfile: () => {},
} as unknown as FeaturesApi

function Probe() {
  useViewport(1)
  return <div className="app"><SettingsPanel target="confirmations" activeRadioId={0}
    scale={100} scaleMode="auto" scaleCap={100} onScaleModeChange={() => {}} onScaleCapChange={() => {}}
    density="standard" onDensityChange={() => {}} onResetLayout={() => {}} features={features} /></div>
}
createRoot(document.getElementById('root')!).render(<Probe />)
