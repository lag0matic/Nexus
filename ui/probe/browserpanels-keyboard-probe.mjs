import assert from 'node:assert/strict'
import { createServer } from 'vite'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chrome } from '../../remote/test/browser-runtime.mjs'

const ui = join(dirname(fileURLToPath(import.meta.url)), '..')
const cacheDir = await mkdtemp(join(tmpdir(), 'nexus-sharing-keyboard-'))
const server = await createServer({ root: ui, configFile: join(ui, 'vite.config.ts'), cacheDir,
  logLevel: 'warn', server: { host: '127.0.0.1', port: 0, strictPort: false } })
await server.listen()
let browser
try {
  browser = await chrome()
  const { targetId } = await browser.call('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await browser.call('Target.attachToTarget', { targetId, flatten: true })
  const call = (method, params = {}) => browser.call(method, params, sessionId)
  const evaluate = async expression => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true })
    assert.ok(!r.exceptionDetails, JSON.stringify(r.exceptionDetails))
    return r.result.value
  }
  const wait = async expression => {
    for (let i = 0; i < 150; i++) { if (await evaluate(expression)) return; await new Promise(r => setTimeout(r, 50)) }
    throw new Error(`Timeout: ${expression}`)
  }
  const key = async (type, code, value, virtual) => call('Input.dispatchKeyEvent', { type, code, key: value,
    windowsVirtualKeyCode: virtual, nativeVirtualKeyCode: virtual })
  const press = async (code, value, virtual) => { await key('keyDown', code, value, virtual); await key('keyUp', code, value, virtual) }
  const click = async selector => {
    const r = await evaluate(`(() => { const e=document.querySelector(${JSON.stringify(selector)}); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2,hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))}; })()`)
    assert.ok(r.hit, `real hit target: ${selector}`)
    await call('Input.dispatchMouseEvent', { type: 'mousePressed', x: r.x, y: r.y, button: 'left', clickCount: 1 })
    await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x: r.x, y: r.y, button: 'left', clickCount: 1 })
  }
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const [width, height] of [[1024, 768], [1920, 1080]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    await call('Page.navigate', { url: `${origin}/probe/browserpanels.html` })
    await wait(`!!document.querySelector('.browser-share-trigger')`)
    await evaluate(`document.querySelector('.browser-share-trigger').focus()`)
    await press('Space', ' ', 32)
    await wait(`!!document.querySelector('[role=dialog]') && !!document.querySelector('.browser-share-actions button:not(:disabled)')`)
    assert.equal((await evaluate('window.__shareProbe()')).down, 0, 'opening by Space must not key')
    await evaluate(`document.querySelector('.browser-share-actions button').focus()`)
    await press('F1', 'F1', 112)
    await press('Space', ' ', 32)
    await wait(`!!document.querySelector('input[value="1234 5678"]')`)
    let state = await evaluate('window.__shareProbe()')
    assert.equal(state.down, 0, 'sharing controls must not key')
    assert.equal(state.macro, 0, 'sharing controls must not send macros')
    assert.deepEqual(state.commands, ['status', 'start'])
    await press('Escape', 'Escape', 27)
    await wait(`!document.querySelector('[role=dialog]')`)
    assert.equal((await evaluate('window.__shareProbe()')).stop, 1, 'Escape still reaches the real global stop hook')
    assert.ok(await evaluate(`document.activeElement === document.querySelector('.browser-share-trigger')`), 'focus restored')

    await evaluate(`document.querySelector('#outside').focus()`)
    await key('keyDown', 'Space', ' ', 32)
    assert.equal((await evaluate('window.__shareProbe()')).keyed, true, 'positive control: an outside press keys the simulated rig')
    await click('.browser-share-trigger')
    await wait(`!!document.querySelector('[role=dialog]')`)
    await evaluate(`document.querySelector('.browser-share-fields input').focus()`)
    await key('keyUp', 'Space', ' ', 32)
    assert.equal((await evaluate('window.__shareProbe()')).keyed, false, 'release crosses the dialog focus change')
    await press('Escape', 'Escape', 27)
    await wait(`!document.querySelector('[role=dialog]')`)
    assert.equal((await evaluate('window.__shareProbe()')).stop, 2)
    console.log(`PASS sharing keyboard ${width}x${height}: Space opens/starts without PTT; no macro; release and Escape stop survive; focus restored`)
  }
  await call('Page.navigate', { url: `${origin}/probe/browserpanels.html?control=1` })
  await wait(`!!document.querySelector('.browser-share-trigger')`)
  await evaluate(`document.querySelector('.browser-share-trigger').focus()`)
  await press('Space', ' ', 32)
  assert.ok((await evaluate('window.__shareProbe()')).down > 0, 'negative control must expose the deliberately unshieldable handler')
  console.log('PASS sharing keyboard CONTROL: detector catches a planted PTT shortcut leak; no real radio used')
} finally {
  await browser?.stop()
  await server.close()
  await rm(cacheDir, { recursive: true, force: true })
}
