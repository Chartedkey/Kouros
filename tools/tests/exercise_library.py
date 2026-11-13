"""Browser workflows for the exercise library; fictional demo data only.
Run Vite, then python tools/tests/exercise_library.py --url http://127.0.0.1:5173
"""
import argparse, asyncio, base64, json
from pathlib import Path
from playwright.async_api import async_playwright, expect
async def main(a):
 out=Path(a.output);out.mkdir(parents=True,exist_ok=True)
 async with async_playwright() as p:
  b=await p.chromium.launch(executable_path='/usr/bin/chromium',args=['--no-sandbox','--disable-dev-shm-usage'])
  page=await b.new_page(viewport={'width':1440,'height':1000});errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
  await page.add_init_script("localStorage.setItem('kouros_tour_v1','1')")
  await page.goto(a.url);await page.get_by_role('button',name='Войти через Windows Hello',exact=True).click()
  await page.locator('header nav').get_by_role('button',name='Тренировки',exact=True).click();await page.get_by_role('button',name='Инвентарь',exact=True).click()
  await page.get_by_role('region',name='Мои комплексы',exact=True).locator('.list-row > button').first.wait_for()
  old=await page.get_by_role('region',name='Мои комплексы',exact=True).locator('.list-row > button').all_text_contents()
  await page.get_by_role('button',name='Открыть библиотеку',exact=True).click();modal=page.get_by_role('dialog',name='Библиотека упражнений',exact=True)
  await modal.locator('.picker-item').first.wait_for();await page.screenshot(path=str(out/'catalog.png'))
  assert await modal.locator('.picker-item').count()<=24
  search=modal.get_by_role('textbox',name='Поиск упражнения');await search.fill('отжимания')
  assert await modal.locator('.picker-item').count()>0
  tile=modal.locator('.picker-item').filter(has=page.get_by_text('Отжимания с колен',exact=True));assert await tile.count()==1
  await tile.get_by_role('button',name='Как делать',exact=True).click()
  info=page.get_by_role('dialog',name='Отжимания с колен',exact=True)
  await info.locator('img').evaluate('(e)=>e.decode()');assert await info.locator('img').evaluate('(e)=>e.naturalWidth')==1536
  await page.screenshot(path=str(out/'exercise.png'));await info.get_by_role('button',name='Закрыть',exact=True).click()
  await tile.get_by_role('button',name='В избранное: Отжимания с колен',exact=True).click()
  await modal.get_by_role('button',name='Избранные',exact=True).click();await search.fill('');await expect(modal.locator('.picker-item')).to_have_count(1)
  await modal.get_by_role('button',name='+ Добавить упражнение',exact=True).click();editor=page.get_by_role('dialog',name='Добавить упражнение',exact=True)
  await editor.get_by_label('Название',exact=True).fill('Моя тяга ленты')
  await editor.get_by_label('Инвентарь',exact=True).select_option('bands')
  await editor.get_by_role('button',name='ОФП для бега',exact=True).click()
  await editor.get_by_label('Выполнение по сторонам',exact=True).select_option('arm')
  await editor.get_by_role('group',name='Основные мышцы',exact=True).get_by_role('button',name='широчайшие',exact=True).click()
  await editor.get_by_label('Описание техники',exact=True).fill('Тянуть к поясу, сохраняя ровный корпус.')
  png=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6LwAAAABJRU5ErkJggg==')
  await editor.locator('input[type=file]').set_input_files({'name':'own.png','mimeType':'image/png','buffer':png})
  await editor.locator('img').evaluate('(e)=>e.decode()')
  await editor.get_by_role('button',name='Сохранить упражнение',exact=True).click();info=page.get_by_role('dialog',name='Моя тяга ленты',exact=True);await info.wait_for()
  assert 'Тянуть к поясу' in await info.inner_text();await info.get_by_role('button',name='Закрыть',exact=True).click()
  await modal.get_by_role('button',name='Мои',exact=True).click();await search.fill('Моя тяга');await expect(modal.locator('.picker-item')).to_have_count(1)
  await modal.get_by_role('button',name='Изменить',exact=True).click();editor=page.get_by_role('dialog',name='Изменить упражнение',exact=True)
  assert await editor.get_by_label('Инвентарь',exact=True).input_value()=='bands';assert await editor.get_by_label('Выполнение по сторонам',exact=True).input_value()=='arm'
  await editor.locator('img').evaluate('(e)=>e.decode()');await editor.get_by_role('button',name='Отмена',exact=True).click()
  await modal.get_by_role('button',name='Каталог',exact=True).click();await search.fill('');await modal.get_by_label('Состав каталога',exact=True).select_option('legacy');await search.fill('atlas')
  assert await modal.locator('.picker-item').count()>0
  for width in [1024,1440]:
   await page.set_viewport_size({'width':width,'height':900});assert await page.evaluate('document.documentElement.scrollWidth<=innerWidth+2')
  await modal.get_by_role('button',name='Закрыть',exact=True).click()
  assert old == await page.get_by_role('region',name='Мои комплексы',exact=True).locator('.list-row > button').all_text_contents() # independent creation did not change any complex
  await page.get_by_role('button',name='Открыть библиотеку',exact=True).click();modal=page.get_by_role('dialog',name='Библиотека упражнений',exact=True)
  await modal.get_by_role('button',name='Избранные',exact=True).click();await expect(modal.locator('.picker-item')).to_have_count(1)
  await modal.get_by_role('button',name='Закрыть',exact=True).click()
  await page.get_by_role('button',name='Все тренировки',exact=True).click()
  await page.get_by_text('Силовая тренировка',exact=True).click()
  strength=page.get_by_role('region',name='Силовая',exact=True)
  await strength.get_by_role('button',name='+ Упражнение',exact=True).click()
  picker=page.get_by_role('dialog',name='Выбери упражнение',exact=True)
  await picker.get_by_role('textbox',name='Поиск упражнения').fill('отжимания с колен')
  await picker.locator('.picker-pick').filter(has=page.get_by_text('Отжимания с колен',exact=True)).click()
  await expect(strength.locator('.exh-name').filter(has_text='Отжимания с колен')).to_have_count(1)
  assert 'new_pushup_knees.png' in await strength.locator('.thumb').last.evaluate('(e)=>e.style.backgroundImage')
  await strength.get_by_label('Повторы',exact=True).last.fill('12')
  await strength.get_by_role('button',name='+ Упражнение',exact=True).click()
  picker=page.get_by_role('dialog',name='Выбери упражнение',exact=True)
  await picker.get_by_role('button',name='+ Добавить упражнение',exact=True).click()
  editor=page.get_by_role('dialog',name='Добавить упражнение',exact=True)
  await editor.get_by_label('Название',exact=True).fill('Новое упражнение из тренировки')
  await editor.get_by_role('button',name='Сохранить упражнение',exact=True).click()
  await expect(strength.locator('.exh-name').filter(has_text='Новое упражнение из тренировки')).to_have_count(1)
  ofp=page.get_by_role('region',name='ОФП и комплексы',exact=True)
  await ofp.get_by_role('button',name='+ ОФП для бега',exact=True).click()
  picker=page.get_by_role('dialog',name='Выбери упражнение',exact=True)
  await picker.get_by_role('textbox',name='Поиск упражнения').fill('короткая стопа')
  await picker.locator('.picker-pick').filter(has=page.get_by_text('Короткая стопа',exact=True)).click()
  await expect(ofp.get_by_role('textbox',name='Секунд',exact=True)).to_have_count(1)
  await ofp.get_by_role('button',name='+ Комплекс',exact=True).click()
  await ofp.get_by_role('button',name='Пример: колени',exact=True).click()
  # Personal complexes continue to resolve their original names/media, without the overlay.
  await expect(ofp.get_by_text('Охотничья собака',exact=True)).to_have_count(1)
  await ofp.get_by_role('button',name='Охотничья собака',exact=True).click()
  legacy=page.get_by_role('dialog',name='Охотничья собака',exact=True)
  await expect(legacy.locator('img')).to_have_count(0)
  await legacy.get_by_role('button',name='Закрыть',exact=True).click()
  await page.screenshot(path=str(out/'workout-library.png'),full_page=True)
  assert not errors,errors
  await b.close();(out/'browser-check.json').write_text(json.dumps({'search':'PASS','full_static_illustration':'PASS','favorites_reopen':'PASS','custom_image_muscles_category_edit':'PASS','legacy_search':'PASS','strength_pick':'PASS','create_and_pick_in_workout':'PASS','ofp_timed_pick':'PASS','complexes_legacy_presentation':'PASS','complexes_preserved':'PASS','widths':[1024,1440],'page_errors':errors},ensure_ascii=False,indent=2));print('Browser workflows PASS')
if __name__=='__main__':
 q=argparse.ArgumentParser();q.add_argument('--url',default='http://127.0.0.1:5173');q.add_argument('--output',default='/workspace/shared/kouros-library');asyncio.run(main(q.parse_args()))
