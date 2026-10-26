import assert from 'node:assert/strict'
import { createServer } from 'vite'
import { mkdir, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { chrome } from '../../remote/test/browser-runtime.mjs'
const ui = join(dirname(fileURLToPath(import.meta.url)), '..')
const server = await createServer({ root: ui, configFile: join(ui, 'vite.config.ts'), logLevel: 'warn', server: { host: '127.0.0.1', port: 0 } })
await server.listen()
let browser
try {
  browser = await chrome()
  const { targetId } = await browser.call('Target.createTarget', { url: 'about:blank' })
  const { sessionId } = await browser.call('Target.attachToTarget', { targetId, flatten: true })
  const call = (method, params = {}) => browser.call(method, params, sessionId)
  browser.on('Runtime.exceptionThrown', ({ exceptionDetails }) => console.error(exceptionDetails.exception?.description ?? exceptionDetails.text))
  await call('Runtime.enable')
  const evaluate = async expression => {
    const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails))
    return r.result.value
  }
  for (const [width, height] of [[1024, 768], [1920, 1080]]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false })
    await call('Page.navigate', { url: `http://127.0.0.1:${server.httpServer.address().port}/probe/ic7300.html` })
    let ready = false
    for (let i = 0; i < 200; i++) {
      ready = await evaluate(`!!document.querySelector('[aria-label="Manual notch position"]')`)
      if (ready) break
      await new Promise(r => setTimeout(r, 100))
    }
    if (!ready) console.error(await evaluate('document.body.innerText'))
    assert.ok(ready, 'raw notch slider mounted')
    const geometry = await evaluate(`(() => {
      const e=document.querySelector('[aria-label="Manual notch position"]');
      e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect();
      return { value:e.value, min:e.min, max:e.max, disabled:e.disabled,
        visible:r.width>80&&r.height>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight,
        hit:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)===e };
    })()`)
    assert.deepEqual({ value: geometry.value, min: geometry.min, max: geometry.max, disabled: geometry.disabled }, { value: '127', min: '0', max: '255', disabled: false })
    assert.ok(geometry.visible && geometry.hit, JSON.stringify(geometry))
    await evaluate(`(() => {const e=document.querySelector('[aria-label="Manual notch position"]'); const r=e.getBoundingClientRect(); window.probeClick={x:r.x+r.width*0.75,y:r.y+r.height/2};})()`)
    const point = await evaluate('window.probeClick')
    await call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 })
    await call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 })
    assert.ok(await evaluate('Number.isInteger(JSON.parse(document.documentElement.dataset.notchCommand || "{}").position)'), 'real slider click sends integer raw position')
    const shot = await call('Page.captureScreenshot', { format: 'png' })
    await mkdir(join(ui, '..', 'target', 'ic7300-probe'), { recursive: true })
    await writeFile(join(ui, '..', 'target', 'ic7300-probe', `${width}x${height}.png`), Buffer.from(shot.data, 'base64'))
    console.log(`PASS IC-7300 ${width}x${height}: raw units, usable slider, hit test, actual input command`)
  }
} finally {
  await browser?.stop()
  await server.close()
}
