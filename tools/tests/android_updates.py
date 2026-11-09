"""Browser tests of the Android update UI with mocked native IPC, not the installer.
Run Vite on port 5175, then python tools/tests/android_updates.py.
"""
import asyncio
from playwright.async_api import async_playwright

HTML = '''<html><body><div id="test"></div><script type="module">
import React from '/node_modules/.vite/deps/react.js';
import ReactDOM from '/node_modules/.vite/deps/react-dom_client.js';
import RefreshRuntime from '/@react-refresh';
RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>(type)=>type;window.__vite_plugin_react_preamble_installed__=true;
const {UpdateBanner,UpdatesSection}=await import('/src/components/Updates.tsx');
window.calls=[];window.result=null;window.fail=false;window.installFail=false;
window.__TAURI_INTERNALS__={invoke:async(cmd)=>{
 calls.push(cmd);
 if(cmd==='mobile_update_check'){if(window.fail)throw Error('offline');return window.result;}
 if(cmd==='mobile_update_install' && window.installFail)throw Error('download failed');
}};
ReactDOM.createRoot(document.getElementById('test')).render(React.createElement(React.Fragment,null,React.createElement(UpdateBanner),React.createElement(UpdatesSection)));
window.ready=true;
</script></body></html>'''

async def main():
 async with async_playwright() as p:
  browser=await p.chromium.launch(executable_path='/usr/bin/chromium',args=['--no-sandbox'])
  page=await browser.new_page(user_agent='Mozilla/5.0 (Linux; Android 16) Chrome/130.0 Mobile')
  await page.route('**/update-test',lambda route:route.fulfill(body=HTML,content_type='text/html'))
  page.on('pageerror', lambda e: print('PAGE ERROR',e,flush=True))
  await page.goto('http://127.0.0.1:5175/update-test')
  await page.wait_for_function('window.ready===true')
  await page.wait_for_timeout(100)
  await page.clock.install()
  await page.wait_for_timeout(100)
  # Recent successful checks must still schedule a check in a long foreground session.
  await page.evaluate("localStorage.setItem('kouros_android_update_checked_v2',String(Date.now()))")
  await page.reload();await page.wait_for_function('window.ready===true')
  await page.clock.fast_forward(5000)
  assert await page.evaluate('calls.length')==0
  await page.clock.fast_forward(30*60*1000)
  await page.wait_for_function("calls.includes('mobile_update_check')")
  # Automatic discovery, failure recovery, and system installation handoff.
  await page.evaluate("window.result={version:'2.0.0-alpha.test',notes:'Исправления'};window.fail=true")
  await page.clock.fast_forward(30*60*1000)
  await page.evaluate('window.fail=false')
  await page.clock.fast_forward(30*60*1000)
  await page.get_by_text('Доступна версия 2.0.0-alpha.test',exact=True).wait_for()
  await page.evaluate('window.installFail=true')
  await page.get_by_role('button',name='Обновить',exact=True).click()
  await page.get_by_text('Не получилось обновить:',exact=False).wait_for()
  await page.evaluate('window.installFail=false')
  await page.get_by_role('button',name='Обновить',exact=True).click()
  await page.get_by_role('status').filter(has_text='Подтвердите установку').wait_for()
  await page.get_by_role('button',name='Позже',exact=True).click()
  # Preferences take effect immediately; manual checks still work when auto is off.
  await page.get_by_role('checkbox').uncheck()
  before=await page.evaluate('calls.length')
  await page.clock.fast_forward(31*60*1000)
  assert await page.evaluate('calls.length')==before
  await page.evaluate('window.result=null')
  await page.get_by_role('button',name='Проверить обновления').click()
  await page.get_by_text('У тебя последняя версия.',exact=True).wait_for()
  await page.get_by_role('checkbox').check()
  await page.clock.fast_forward(5000)
  assert await page.evaluate('calls.length')>before
  await browser.close()
 print('PASS: scheduled checks, network retry, install retry, Android handoff, preference and manual check')

asyncio.run(main())
