export const OBSERVER_PANELS = [
  { id: 'pota', label: 'POTA' },
  { id: 'conditions', label: 'Conditions' },
  { id: 'needed', label: 'Needed' },
  { id: 'spots', label: 'Spots' },
  { id: 'bandmapPhone', label: 'Phone band map' },
  { id: 'bandmapCw', label: 'CW band map' },
  { id: 'memories', label: 'Memories' },
  { id: 'dxped', label: 'DXpeditions' },
  { id: 'sats', label: 'Satellites' },
  { id: 'activation', label: 'Activation' },
  { id: 'operate', label: 'Operate' },
  { id: 'operatemap', label: 'Station map' },
  { id: 'waterfall', label: 'Waterfall' },
  { id: 'sstvviewer', label: 'SSTV picture' },
  { id: 'fieldday', label: 'Contest scoreboard' },
  { id: 'contestlog', label: 'Contest log' },
  { id: 'contestscreen', label: 'Contest screen' },
  { id: 'fdclub', label: 'Club band board' },
] as const

export type ObserverPanel = typeof OBSERVER_PANELS[number]['id']
export function observerPanel(slug: string | undefined): ObserverPanel {
  if (slug === 'connect') return 'conditions'
  return OBSERVER_PANELS.find(p => p.id === slug)?.id ?? 'pota'
}

export const POTA_BANDS = ['160m', '80m', '60m', '40m', '30m', '20m', '17m', '15m', '12m', '10m', '6m', '4m', '2m', '1.25m', '70cm']
export function filterMode(mode: string | null): string {
  return mode === 'USB' || mode === 'LSB' ? 'SSB' : mode ?? 'Unknown'
}
