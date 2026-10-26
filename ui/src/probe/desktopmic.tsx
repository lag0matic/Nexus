// Isolated browser fixture: no native radio, real devices, or transmit commands.
import '../styles.css'
import { createRoot } from 'react-dom/client'
import { SettingsPanel } from '../components/SettingsPanel'
import { useViewport } from '../useViewport'
import type { FeaturesApi } from '../useFeatures'
import defaults from '../components/__fixtures__/defaultSettings.json'

document.documentElement.dataset.theme = 'dark'
let settings = { ...defaults, mycall: 'W9XYZ', phoneDesktopMic: false, voiceMicDevice: 'Desktop microphone' }
window.__TAURI_INTERNALS__ = {
  invoke: async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
    if (command === 'set_settings') {
      settings = { ...settings, ...args?.settings as object }
      document.documentElement.dataset.micSaved = JSON.stringify({ desktop: settings.phoneDesktopMic, device: settings.voiceMicDevice })
      return {} as T
    }
    const answers: Record<string, unknown> = {
      get_settings: settings, get_rig_models: [], get_all_rig_models: [],
      get_serial_ports_detailed: [], get_band_plan: [], get_credentials_status: [],
      get_connection_log: [], detect_rigs: [], app_version: '1.19.0',
      get_audio_devices: {
        input: ['Desktop microphone', 'Radio receive'].map(name => ({ name, label: name })),
        output: [{ name: 'Radio transmit', label: 'Radio transmit' }],
      },
    }
    if (!(command in answers)) throw new Error(`Unexpected probe command: ${command}`)
    return answers[command] as T
  },
}
const features = { enabled: () => true, setEnabled: () => {}, all: () => [], profile: 'full', setProfile: () => {} } as unknown as FeaturesApi
function Probe() {
  useViewport(1)
  return <div className="app"><SettingsPanel target="phone" activeRadioId={0}
    scale={100} scaleMode="auto" scaleCap={100} onScaleModeChange={() => {}} onScaleCapChange={() => {}}
    density="standard" onDensityChange={() => {}} onResetLayout={() => {}} features={features} /></div>
}
createRoot(document.getElementById('root')!).render(<Probe />)
