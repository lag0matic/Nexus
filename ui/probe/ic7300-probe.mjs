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
    for (const name of ['PBT inner', 'PBT outer', 'Notch width', 'Center both PBT controls']) {
      const geometry = await evaluate(`(() => {
        const e=document.querySelector('[aria-label="${name}"]');
        if(!e) return null; e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect();
        return {w:r.width,h:r.height,x:r.x,y:r.y,right:r.right,bottom:r.bottom,
          hit:e.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))};
      })()`)
      assert.ok(geometry?.w > 0 && geometry.h > 0 && geometry.x >= 0 && geometry.right <= width && geometry.hit,
        `${name}: ${JSON.stringify(geometry)}`)
    }
    await evaluate(`document.querySelector('[aria-label="PBT inner"]').focus({preventScroll:true})`)
    await call('Input.dispatchKeyEvent', { type:'keyDown', key:'ArrowRight', code:'ArrowRight', windowsVirtualKeyCode:39 })
    await call('Input.dispatchKeyEvent', { type:'keyUp', key:'ArrowRight', code:'ArrowRight', windowsVirtualKeyCode:39 })
    await new Promise(r=>setTimeout(r,100))
    assert.deepEqual(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1)'),
      { command:'set_ic7300_rx_control', args:{control:0,value:129} })
    await evaluate(`document.querySelector('[aria-label="Notch width"]').focus({preventScroll:true})`)
    await call('Input.dispatchKeyEvent', { type:'keyDown', key:'ArrowDown', code:'ArrowDown', windowsVirtualKeyCode:40 })
    await call('Input.dispatchKeyEvent', { type:'keyUp', key:'ArrowDown', code:'ArrowDown', windowsVirtualKeyCode:40 })
    await new Promise(r=>setTimeout(r,100))
    assert.deepEqual(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1)'),
      { command:'set_ic7300_rx_control', args:{control:2,value:1} })
    const clickButton = async name => {
      const point = await evaluate(`(() => {
        const e=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='${name}');
        e.scrollIntoView({block:'center'}); const r=e.getBoundingClientRect();
        return {x:r.x+r.width/2,y:r.y+r.height/2,disabled:e.disabled};
      })()`)
      assert.equal(point.disabled, false, `${name} is usable`)
      await call('Input.dispatchMouseEvent', { type: 'mousePressed', x:point.x, y:point.y, button:'left',clickCount:1 })
      await call('Input.dispatchMouseEvent', { type: 'mouseReleased', x:point.x, y:point.y, button:'left',clickCount:1 })
      await new Promise(r=>setTimeout(r,100))
    }
    await clickButton('Tune')
    assert.equal(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1).command'), 'set_tuner')
    assert.equal(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1).args.on'), true)
    await clickButton('Tune')
    assert.equal(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1).args.on'), false)
    await clickButton('ATU')
    assert.equal(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).at(-1).command'), 'atu_tune')
    assert.equal(await evaluate('JSON.parse(document.documentElement.dataset.rxCommands).some(c=>c.command==="set_tune")'), false)
    const shot = await call('Page.captureScreenshot', { format: 'png' })
    await mkdir(join(ui, '..', 'target', 'ic7300-probe'), { recursive: true })
    await writeFile(join(ui, '..', 'target', 'ic7300-probe', `${width}x${height}.png`), Buffer.from(shot.data, 'base64'))
    console.log(`PASS IC-7300 ${width}x${height}: raw units, usable slider, hit test, actual input command`)
  }
} finally {
  await browser?.stop()
  await server.close()
}
