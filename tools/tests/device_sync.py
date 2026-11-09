"""UI checks with synthetic IPC; data/wire invariants live in tools/peer-tests."""
import asyncio
from pathlib import Path
from playwright.async_api import async_playwright

HTML='''<html><head><meta charset="utf-8"></head><body style="background:#0b0d10"><div id="test" class="compact" style="max-width:680px;margin:auto;padding:12px"></div><script type="module">
import React from '/node_modules/.vite/deps/react.js';
import ReactDOM from '/node_modules/.vite/deps/react-dom_client.js';
import RefreshRuntime from '/@react-refresh';
RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>(type)=>type;window.__vite_plugin_react_preamble_installed__=true;
window.rpc=[];window.fail=false;window.state={hostRunning:false,address:'192.168.1.10:8766',paired:false,ready:false,remote:'192.168.1.10:8766',lastSync:null,error:null,busy:false,conflicts:0,counts:{}};
window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{rpc.push({cmd,args});
 if(cmd==='peer_status')return {...window.state};
 if(cmd==='peer_enable'){window.state.hostRunning=true;return {code:'1234567890abcdef1234567890abcdef',address:state.address,expires:1900000000};}
 if(cmd==='peer_pair'){if(args.code==='wrong')throw Error('Код не подошёл');window.state.ready=true;window.state.paired=true;return {applied:123,backup:'fictional-backup.db'};}
 if(cmd==='peer_address'){window.state.remote=args.address;return null;}
 if(cmd==='peer_sync'){if(window.fail)throw Error('Нет соединения с Windows');return {sent:2,received:3};}
 if(cmd==='peer_conflicts')return window.conflictFixture||[];
 if(cmd==='peer_resolve'){window.conflictFixture=[];return null;}
 if(cmd==='peer_forget'){window.state.ready=false;window.state.paired=false;window.state.hostRunning=false;return null;}
 return 1;
}};
await import('/src/styles.css');
const {DeviceSync}=await import('/src/components/DeviceSync.tsx');
ReactDOM.createRoot(document.getElementById('test')).render(React.createElement(DeviceSync));
window.ready=true;
</script></body></html>'''
ANDROID='Mozilla/5.0 (Linux; Android 16) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36'
async def check_width(page,width):
 actual=await page.evaluate('document.documentElement.scrollWidth')
 assert actual<=width+2,(actual,width)
async def main():
 async with async_playwright() as p:
  browser=await p.chromium.launch(executable_path='/usr/bin/chromium',args=['--no-sandbox'])
  page=await browser.new_page(viewport={'width':360,'height':800},user_agent=ANDROID)
  errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
  await page.goto('http://127.0.0.1:5175/',wait_until='networkidle')
  await page.route('**/peer-test',lambda r:r.fulfill(body=HTML,content_type='text/html'))
  await page.goto('http://127.0.0.1:5175/peer-test');await page.wait_for_function('window.ready===true')
  button=page.get_by_role('button',name='Перенести историю с Windows',exact=True)
  assert not await button.is_enabled()
  await page.get_by_label('Адрес Windows',exact=True).fill('192.168.1.10:8766')
  await page.get_by_label('Код сопряжения',exact=True).fill('wrong')
  assert not await button.is_enabled()
  await page.get_by_role('checkbox').check();await button.click()
  await page.get_by_role('alert').filter(has_text='Код не подошёл').wait_for()
  assert not await page.evaluate('state.ready')
  await check_width(page,360)
  await page.get_by_label('Код сопряжения',exact=True).fill('1234567890abcdef1234567890abcdef')
  await button.click();await page.get_by_text('История перенесена:',exact=False).wait_for()
  assert await page.get_by_label('Код сопряжения',exact=True).count()==0
  await page.get_by_text('Изменить адрес компьютера',exact=True).click()
  await page.get_by_label('Адрес Windows',exact=True).fill('192.168.1.20:8766')
  await page.get_by_role('button',name='Сохранить адрес',exact=True).click()
  await page.get_by_text('Адрес компьютера сохранён.',exact=True).wait_for()
  assert await page.evaluate('state.ready')
  await page.evaluate('window.fail=true')
  await page.get_by_role('button',name='Синхронизировать сейчас',exact=True).click()
  await page.get_by_role('alert').filter(has_text='Нет соединения').wait_for()
  await page.evaluate('window.fail=false')
  await page.get_by_role('button',name='Синхронизировать сейчас',exact=True).click()
  await page.get_by_text('Передано изменений:',exact=False).wait_for()
  await page.get_by_role('button',name='Проверить конфликты изменений',exact=False).click()
  await page.get_by_text('Неразобранных конфликтов нет.',exact=True).wait_for()
  await page.evaluate('''window.conflictFixture=[{id:1,at:1900000000,table:'meds',key:'[1]',local:JSON.stringify({name:'Fixture supplement',dose:1}),remote:JSON.stringify({row:{name:'Fixture supplement',dose:2}})}]''')
  await page.get_by_role('button',name='Проверить конфликты изменений',exact=False).click()
  await page.get_by_text('Разные изменения: Препарат',exact=True).wait_for()
  await page.get_by_text('Сравнить варианты',exact=True).click()
  await page.locator('pre').filter(has_text='Доза: 2').wait_for()
  await page.get_by_role('button',name='Оставить текущий',exact=True).click()
  await page.get_by_text('Неразобранных конфликтов нет.',exact=True).wait_for()
  await check_width(page,360)
  await page.get_by_role('button',name='Отключить сопряжение',exact=True).click()
  assert await page.evaluate("rpc.filter(x=>x.cmd==='peer_forget').length")==0
  await page.get_by_role('button',name='Подтвердить отключение',exact=True).click()
  await page.get_by_text('Сопряжение отключено. История сохранена.',exact=True).wait_for()
  desktop=await browser.new_page(viewport={'width':1100,'height':900})
  desktop.on('pageerror',lambda e:errors.append(str(e)))
  await desktop.route('**/peer-test',lambda r:r.fulfill(body=HTML,content_type='text/html'))
  await desktop.goto('http://127.0.0.1:5175/peer-test');await desktop.wait_for_function('window.ready===true')
  await desktop.get_by_role('button',name='Показать код сопряжения',exact=True).click()
  await desktop.get_by_text('1234 5678 90ab cdef 1234 5678 90ab cdef',exact=True).wait_for()
  assert await desktop.get_by_role('button',name='Перенести историю с Windows',exact=True).count()==0
  # Full browser app: Android must reach its main screen without an authentication action.
  app=await browser.new_page(viewport={'width':412,'height':915},user_agent=ANDROID)
  app.on('pageerror',lambda e:errors.append(str(e)))
  await app.add_init_script("localStorage.setItem('kouros_tour_v1','1')")
  await app.goto('http://127.0.0.1:5175/',wait_until='networkidle')
  await app.locator('.wrap').wait_for()
  assert await app.get_by_role('button',name='Отпечаток или PIN-код',exact=True).count()==0
  await app.locator('.header-c').get_by_role('button',name='Настройки и профиль',exact=True).click()
  await app.locator('nav.subtabs').get_by_role('button',name='Данные и безопасность',exact=True).click()
  await app.get_by_text('Обычный запуск приложения',exact=True).wait_for()
  assert await app.get_by_role('switch',name='Вход на Android всегда защищён').count()==0
  await check_width(app,412)
  output=Path('/workspace/scratch/android-alpha3');output.mkdir(parents=True,exist_ok=True)
  await app.screenshot(path=str(output/'normal-start-settings.png'))
  # Render the new vector source for visual review; no personal data is involved.
  icon=await browser.new_page(viewport={'width':512,'height':512},device_scale_factor=1)
  svg=Path('tools/android/icon/brand-icon.svg').read_text().replace('width="1024" height="1024"','width="512" height="512"')
  await icon.set_content('<style>body{margin:0;background:#0b0d10}</style>'+svg)
  await icon.screenshot(path=str(output/'brand-icon.png'))
  assert not errors,errors
  await browser.close()
 print('PASS: consent, pairing failure/retry, sync failure/retry, conflict view, forget confirmation, Windows pairing code, Android ordinary startup, icon preview')
asyncio.run(main())
