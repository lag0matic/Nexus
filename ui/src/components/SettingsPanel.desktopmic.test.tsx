// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { SettingsPanel } from './SettingsPanel'
import type { FeaturesApi } from '../useFeatures'
import defaults from './__fixtures__/defaultSettings.json'

const api = vi.hoisted(() => {
  const spies: Record<string, ReturnType<typeof vi.fn>> = {}
  const get = (name: string) => spies[name] ??= vi.fn(async () => null)
  return { get, spies }
})
vi.mock('../api', async (original) => {
  const actual = await original<Record<string, unknown>>()
  return Object.fromEntries(Object.entries(actual).map(([k, v]) => [k, typeof v === 'function' ? api.get(k) : v]))
})
vi.mock('../toast', () => ({ pushToast: vi.fn(), withErrorToast: vi.fn(async (fn: () => Promise<unknown>) => fn()) }))
const features = { enabled: () => true, setEnabled: vi.fn(), all: () => [], profile: 'full', setProfile: vi.fn() } as unknown as FeaturesApi

beforeEach(() => {
  for (const spy of Object.values(api.spies)) spy.mockReset().mockResolvedValue(null)
  for (const name of ['getRigModels', 'getAllRigModels', 'getSerialPortsDetailed', 'getBandPlan', 'detectRigs', 'getConnectionLog']) api.get(name).mockResolvedValue([])
  api.get('getAudioDevices').mockResolvedValue({ input: ['Desktop mic', 'Radio receive'].map(name => ({ name, label: name })), output: [{ name: 'Radio transmit', label: 'Radio transmit' }] })
  api.get('getCredentialsStatus').mockResolvedValue({})
  api.get('appVersion').mockResolvedValue('1.19.0')
  api.get('getSettings').mockResolvedValue({ ...defaults, mycall: 'W9XYZ', phoneDesktopMic: false, voiceMicDevice: 'Desktop mic' })
})
afterEach(cleanup)

it('keeps rig mic as default and saves the explicit desktop source through the normal form', async () => {
  render(<SettingsPanel activeRadioId={0} scale={100 as never} scaleMode={'auto' as never} scaleCap={100 as never}
    onScaleModeChange={() => {}} onScaleCapChange={() => {}} density={'comfortable' as never}
    onDensityChange={() => {}} onResetLayout={() => {}} features={features} />)
  fireEvent.click(await screen.findByRole('tab', { name: 'Phone' }))
  const source = await screen.findByRole('combobox', { name: 'Live voice source' })
  expect((source as HTMLSelectElement).value).toBe('rig')
  fireEvent.change(source, { target: { value: 'desktop' } })
  expect((screen.getByRole('combobox', { name: /^Desktop microphone input/ }) as HTMLSelectElement).value).toBe('Desktop mic')
  expect(screen.getByRole('option', { name: 'Select a microphone' })).toBeTruthy()
  fireEvent.click(screen.getByRole('button', { name: 'Save' }))
  await vi.waitFor(() => expect(api.get('setSettings')).toHaveBeenCalled())
  const calls = api.get('setSettings').mock.calls
  expect(calls[calls.length - 1]?.[0]).toMatchObject({ phoneDesktopMic: true, voiceMicDevice: 'Desktop mic' })
})
