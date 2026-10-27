// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Ic7300RxControls } from './Ic7300RxControls'
import type { RadioStatus } from '../types'
const api = vi.hoisted(() => ({ setIc7300RxControl: vi.fn(async () => ({})) }))
vi.mock('../api', () => api)
vi.mock('../toast', () => ({ pushToast: vi.fn() }))
afterEach(() => { cleanup(); vi.clearAllMocks() })
describe('IC-7300 receive controls', () => {
  it('does not write defaults or invent capabilities', () => {
    render(<Ic7300RxControls radio={{ catOk: true } as RadioStatus} />)
    expect(screen.queryByRole('slider')).toBeNull()
    expect(screen.queryByRole('combobox')).toBeNull()
    expect(api.setIc7300RxControl).not.toHaveBeenCalled()
  })
  it('writes independent raw PBT positions, centers both, and selects notch width', async () => {
    render(<Ic7300RxControls radio={{ catOk: true, ic7300RxControls: [128, 128, 0] } as RadioStatus} />)
    fireEvent.change(screen.getByRole('slider', { name: 'PBT inner' }), { target: { value: '200' } })
    await waitFor(() => expect(api.setIc7300RxControl).toHaveBeenCalledWith(0, 200))
    fireEvent.change(screen.getByRole('slider', { name: 'PBT outer' }), { target: { value: '70' } })
    await waitFor(() => expect(api.setIc7300RxControl).toHaveBeenCalledWith(1, 70))
    fireEvent.click(screen.getByRole('button', { name: 'Center both PBT controls' }))
    await waitFor(() => expect(api.setIc7300RxControl).toHaveBeenCalledWith(0, 128))
    expect(api.setIc7300RxControl).toHaveBeenCalledWith(1, 128)
    fireEvent.change(screen.getByRole('combobox', { name: 'Notch width' }), { target: { value: '2' } })
    await waitFor(() => expect(api.setIc7300RxControl).toHaveBeenCalledWith(2, 2))
  })
  it('follows front-panel readback without writing it back to the radio', () => {
    const view = render(<Ic7300RxControls radio={{ catOk: true, ic7300RxControls: [128, 128, 0] } as RadioStatus} />)
    view.rerender(<Ic7300RxControls radio={{ catOk: true, ic7300RxControls: [190, 70, 2] } as RadioStatus} />)
    expect((screen.getByRole('slider', { name: 'PBT inner' }) as HTMLInputElement).value).toBe('190')
    expect((screen.getByRole('combobox', { name: 'Notch width' }) as HTMLSelectElement).value).toBe('2')
    expect(api.setIc7300RxControl).not.toHaveBeenCalled()
  })
})
