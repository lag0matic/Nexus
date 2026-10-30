// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { MonitorPanel } from './MonitorPanel'

vi.mock('../components/MapView', () => ({ MapView: ({ selectedCall, onSelectCall }: {
  selectedCall: string | null; onSelectCall: (call: string) => void
}) => <button type="button" data-selected={selectedCall ?? ''} onClick={() => onSelectCall('W9XYZ')}>Station marker</button> }))
afterEach(cleanup)

it('highlights a station locally without sending a listen or QSO command', () => {
  const listen = vi.fn()
  render(<MonitorPanel data={{ panel: 'operatemap', groups: [], metrics: [], media: { stations: [] } }}
    query="" disabled={false} onListen={listen} />)
  const marker = screen.getByRole('button', { name: 'Station marker' })
  fireEvent.click(marker)
  expect(marker.getAttribute('data-selected')).toBe('W9XYZ')
  expect(listen).not.toHaveBeenCalled()
})
