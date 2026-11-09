"""Browser regression for runaway tab bars (requires Python Playwright + Chromium).

Run Vite, then: python tools/tests/android_layout.py --url http://127.0.0.1:5175
Uses fictional browser demo data only; does not exercise Android native APIs.
"""
import argparse
import asyncio
import json
import re
from pathlib import Path
from playwright.async_api import async_playwright

ANDROID = 'Mozilla/5.0 (Linux; Android 16) AppleWebKit/537.36 Chrome/130.0.0.0 Mobile Safari/537.36'
DESKTOP = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36'

async def audit(browser, args, width, height, mobile):
    page = await browser.new_page(viewport={'width': width, 'height': height}, user_agent=ANDROID if mobile else DESKTOP)
    await page.add_init_script("localStorage.setItem('kouros_tour_v1','1')")
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    await page.goto(args.url, wait_until='networkidle')
    login = page.get_by_role('button', name='Отпечаток или PIN-код' if mobile else 'Войти через Windows Hello')
    if await login.count(): await login.click()
    await page.locator('.wrap').wait_for()
    await page.evaluate('document.fonts.ready')
    results = []

    async def show(name):
        await page.evaluate('window.scrollTo(0,0)')
        if mobile:
            nav = page.locator('.bnav')
            direct = nav.get_by_role('button', name=name, exact=True)
            if await direct.count():
                await direct.click()
            elif name == 'Настройки':
                await page.locator('.header-c').get_by_role('button', name='Настройки и профиль', exact=True).click()
            else:
                await nav.get_by_role('button', name='Ещё', exact=True).click()
                await page.locator('.more-list').get_by_role('button', name=re.compile('^' + re.escape(name))).click()
        else:
            if name == 'Настройки':
                await page.locator('header').get_by_role('button', name='Настройки и профиль', exact=True).click()
            elif name == 'Справка':
                await page.get_by_role('button', name='Открыть справку', exact=True).click()
            else:
                await page.locator('header nav').get_by_role('button', name=name, exact=True).click()
        await page.wait_for_timeout(150)

    async def measure(label):
        # Allow async demo data, fonts, chart measurement and observers to settle first.
        await page.wait_for_timeout(650)
        samples = []
        for _ in range(20):
            samples.append(await page.evaluate('''() => ({
                height: document.documentElement.scrollHeight,
                width: document.documentElement.scrollWidth,
                y: scrollY,
                tabs: [...document.querySelectorAll('.subtabs')].map(e => ({
                    height: e.getBoundingClientRect().height,
                    top: e.getBoundingClientRect().top + scrollY
                }))
            })'''))
            if mobile:
                assert all(t['height'] <= 65 for t in samples[-1]['tabs']), f'{label}: stretched tab bar {samples[-1]["tabs"]}'
            if len(samples) >= 3 and max(s['height'] for s in samples[-3:]) - min(s['height'] for s in samples[-3:]) <= 4:
                break
            await page.wait_for_timeout(250)
        row = {'viewport': f'{width}x{height}', 'screen': label, 'samples': samples}
        results.append(row)
        if args.output:
            (Path(args.output) / f'{width}-results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2))
        heights = [s['height'] for s in samples[-3:]]
        assert max(heights) - min(heights) <= 4, f'{label}: unstable page height {heights}'
        assert samples[-1]['width'] <= width + 2, f'{label}: page overflows horizontally ({samples[-1]["width"]} > {width})'
        if mobile:
            for sample in samples:
                assert all(t['height'] <= 65 for t in sample['tabs']), f'{label}: stretched tab bar {sample["tabs"]}'
        print(f'{width}x{height}: {label} — stable', flush=True)

    for name in ([] if args.empty_only else args.screens or ['Сегодня', 'Здоровье', 'Тренировки', 'Питание', 'Психология', 'Тренер', 'Настройки', 'Справка']):
        await show(name)
        await measure(name)
        if name == 'Тренер':
            skip = page.get_by_role('button', name='Пропустить, заполню потом', exact=True)
            if await skip.count():
                await skip.click()
                await measure('Тренер / после знакомства')
        if args.output and name in ['Здоровье', 'Тренировки']:
            await page.evaluate('window.scrollTo(0,0)')
            await page.screenshot(path=str(Path(args.output) / f'{width}-{name}.png'))
        subs = await page.locator('nav.subtabs button').all_text_contents()
        for sub in subs:
            await page.locator('nav.subtabs').get_by_role('button', name=sub, exact=True).click()
            await measure(name + ' / ' + sub)
        if name == 'Тренировки':
            await page.get_by_role('button', name='⇅ Импорт / экспорт', exact=True).click()
            await measure('Тренировки / Импорт и экспорт')
            await page.locator('.drop').click()
            await measure('Тренировки / Предпросмотр импорта (демо)')
            commit = page.locator('section[aria-label="Импорт тренировок"] button.btn-green')
            if await commit.is_enabled():
                await commit.click()
                await measure('Тренировки / Импорт завершён (демо)')
            else:
                # The shipped demo already contains this archive: no duplicate import.
                assert 'Нечего импортировать' in await commit.inner_text()

            await page.locator('nav.subtabs').get_by_role('button', name='Все тренировки', exact=True).click()
            await page.locator('button.tr-act').first.wait_for()
            for index in range(min(3, await page.locator('button.tr-act').count())):
                await page.locator('button.tr-act').nth(index).click()
                await measure(f'Тренировка / Карточка {index + 1}')
                await page.locator('button.backbtn').click()
        if name == 'Справка':
            chapters = await page.locator('.help-nav-item').all_text_contents()
            for chapter in chapters:
                await page.locator('.help-nav-item').filter(has_text=chapter).click()
                await measure('Справка / ' + chapter)


    if mobile and not args.empty_only:
        await page.locator('.bnav').get_by_role('button', name='Ещё', exact=True).click()
        await measure('Ещё')
        # A pending targeted navigation must not drag a later, unrelated page.
        await page.locator('.more-list').get_by_role('button', name=re.compile('^Откуда брать данные')).click()
        await page.locator('.bnav').get_by_role('button', name='Здоровье', exact=True).click()
        await page.wait_for_timeout(4500)
        await measure('Здоровье / после отменённого перехода к карточке')
    if args.empty_only:
        # The first APK has no imported health data yet. Exercise that layout too.
        await show('Сегодня')
        await page.evaluate('''async () => {
            const {api} = await import('/src/lib/api.ts');
            api.healthOverview = async () => ({stats: {rows: 0, metrics: 0, firstDate: null, lastDate: null}, latest: []});
        }''')
        await show('Здоровье')
        await page.get_by_text('Данных пока нет', exact=True).wait_for()
        await measure('Здоровье / Без данных')
    assert not errors, errors
    await page.close()
    return results

async def main(args):
    if args.output:
        Path(args.output).mkdir(parents=True, exist_ok=True)
    async with async_playwright() as p:
        browser = await p.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox', '--disable-dev-shm-usage'])
        results = []
        try:
            semaphore = asyncio.Semaphore(args.jobs)
            async def run(viewport):
                async with semaphore:
                    return await audit(browser, args, *viewport)
            batches = await asyncio.gather(*(run(v) for v in [(360, 800, True), (412, 915, True), (915, 412, True), (1440, 1000, False)]))
            results = [row for batch in batches for row in batch]
        finally:
            await browser.close()
        if args.output:
            (Path(args.output) / 'results.json').write_text(json.dumps(results, ensure_ascii=False, indent=2))
        print(f'PASS: {len(results)} screen/viewport checks', flush=True)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:5175/')
    parser.add_argument('--chromium', default='/usr/bin/chromium')
    parser.add_argument('--output')
    parser.add_argument('--screens', nargs='+', help='Check selected main screens instead of the full application')
    parser.add_argument('--jobs', type=int, choices=range(1, 5), default=1, help='Independent browser contexts to run concurrently')
    parser.add_argument('--empty-only', action='store_true', help='Check the health screen with no imported data')
    asyncio.run(main(parser.parse_args()))
