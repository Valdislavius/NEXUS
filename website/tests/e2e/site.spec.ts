import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { pagePaths } from '../../src/config/site';
import { release } from '../../src/config/release';
import { mkdir } from 'node:fs/promises';
const widths = [360, 390, 768, 1024, 1440, 1920];
const screenshotDirectory = process.env.E2E_BASE_URL ? 'qa/docker-screenshots' : 'qa/screenshots';
for (const width of widths) {
  test(`Все страницы при ширине ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1000 });
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(`${message.text()} ${message.location().url}`); });
    for (const path of pagePaths) {
      const response = await page.goto(path);
      expect(response?.status()).toBe(200);
      await expect(page.locator('main h1')).toBeVisible();
      expect(await page.locator('main h1').count()).toBe(1);
      expect(await page.locator('html').getAttribute('lang')).toBe('ru');
      expect(await page.locator('meta[name="description"]').getAttribute('content')).toBeTruthy();
      expect(await page.locator('meta[property="og:title"]').getAttribute('content')).toContain('NEXUS');
      await page.evaluate(() => document.fonts.ready);
      const dimensions = await page.evaluate(() => ({ viewport: document.documentElement.clientWidth, content: document.documentElement.scrollWidth }));
      expect(dimensions.content, `${path} overflow at ${width}px`).toBeLessThanOrEqual(dimensions.viewport);
      if (width === 390 || width === 1440) {
        await mkdir(screenshotDirectory, { recursive: true });
        for (const image of await page.locator('img').all()) { if (await image.isVisible()) await image.scrollIntoViewIfNeeded(); }
        await expect.poll(() => page.locator('img').evaluateAll(images => images.filter(image => !image.closest('dialog')).every(image => (image as HTMLImageElement).complete && (image as HTMLImageElement).naturalWidth > 0))).toBe(true);
        await page.evaluate(() => window.scrollTo(0, 0));
        if (path === '/') await page.screenshot({ path: `${screenshotDirectory}/home-viewport-${width}.png` });
        await page.screenshot({ path: `${screenshotDirectory}/${path === '/' ? 'home' : path.slice(1)}-${width}.png`, fullPage: true });
      }
    }
    expect(errors).toEqual([]);
  });
}
test('Все внутренние ссылки и якоря разрешаются', async ({ page, request, baseURL }) => {
  if (!baseURL) throw new Error('A base URL is required to validate internal links.');
  const origin = new URL(baseURL).origin;
  const hrefs = new Set<string>();
  for (const path of pagePaths) {
    await page.goto(path);
    const links = await page.locator('a[href]').evaluateAll(anchors => anchors.map(anchor => (anchor as HTMLAnchorElement).href));
    for (const link of links) if (new URL(link).origin === origin) hrefs.add(link);
  }
  for (const href of hrefs) {
    const url = new URL(href);
    expect((await request.get(url.pathname)).ok(), url.pathname).toBeTruthy();
    if (url.hash) { await page.goto(href); expect(await page.locator(`[id="${decodeURIComponent(url.hash.slice(1))}"]`).count(), href).toBe(1); }
  }
});
test('Мобильное меню: открытие, Escape, переход, активный пункт', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  const open = page.getByRole('button', { name: 'Открыть меню' });
  await open.click();
  await expect(page.getByRole('navigation', { name: 'Мобильная навигация' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Закрыть меню' })).toHaveAttribute('aria-expanded', 'true');
  await page.keyboard.press('Escape');
  await expect(open).toBeFocused();
  await expect(page.getByRole('navigation', { name: 'Мобильная навигация' })).toBeHidden();
  await open.click();
  await page.getByRole('navigation', { name: 'Мобильная навигация' }).getByRole('link', { name: 'Документация' }).click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(open).toHaveAttribute('aria-expanded', 'false');
  await open.click();
  await expect(page.getByRole('navigation', { name: 'Мобильная навигация' }).getByRole('link', { name: 'Документация' })).toHaveAttribute('aria-current', 'page');
  await page.getByRole('button', { name: 'Закрыть меню' }).click();
  await expect(open).toHaveAttribute('aria-expanded', 'false');
});
test('Интерфейс: честный статус подготовки без личных снимков', async ({ page, request }) => {
  await page.goto('/');
  const placeholder = page.getByTestId('interface-preview');
  await expect(placeholder.getByRole('heading')).toHaveText('Снимки Desktop 1.0 будут добавлены после проверки интерфейса');
  await expect(placeholder.getByRole('link')).toHaveAttribute('href', /^\/roadmap\/?#desktop-progress$/);
  expect(await page.locator('img[src*="/images/"]').count()).toBe(0);
  expect(await page.getByRole('dialog').count()).toBe(0);
  for (const name of ['overview', 'cpu', 'storage']) expect((await request.get('/images/' + name + '.webp')).status()).toBe(404);
});
test('Загрузки, хэши, команда PowerShell и безопасные инструкции', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/download');
  await expect(page.getByRole('link', { name: 'Скачать установщик', exact: true })).toHaveAttribute('href', release.assets.installer.url);
  await expect(page.getByRole('link', { name: 'Скачать Portable', exact: true })).toHaveAttribute('href', release.assets.portable.url);
  for (const asset of Object.values(release.assets)) {
    await page.getByRole('button', { name: `Копировать SHA-256 ${asset.label}`, exact: true }).click();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(asset.sha256);
  }
  await page.getByRole('button', { name: 'Копировать команду проверки SHA-256' }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('Get-FileHash -Algorithm SHA256');
});
test('Навигация на desktop, уникальные title и description, переход клавиатурой', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto('/');
  await page.keyboard.press('Tab');
  await expect(page.getByRole('link', { name: 'Перейти к содержимому' })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('main')).toBeFocused();
  const titles = new Set<string>();
  const descriptions = new Set<string>();
  for (const path of pagePaths) {
    await page.goto(path);
    titles.add(await page.title());
    descriptions.add((await page.locator('meta[name="description"]').getAttribute('content')) ?? '');
    expect(await page.locator('link[rel="canonical"]').count()).toBe(0);
  }
  expect(titles.size).toBe(pagePaths.length);
  expect(descriptions.size).toBe(pagePaths.length);
  for (const name of ['Возможности', 'Загрузки', 'Документация', 'Roadmap']) {
    await page.getByRole('navigation', { name: 'Основная навигация' }).getByRole('link', { name }).click();
    await expect(page.getByRole('navigation', { name: 'Основная навигация' }).getByRole('link', { name })).toHaveAttribute('aria-current', 'page');
  }
});
test('Документация: 19 разделов и раскрытие решения проблемы', async ({ page }) => {
  await page.goto('/docs');
  expect(await page.locator('.docs-section').count()).toBe(19);
  const summary = page.getByText('Portable не запускается', { exact: true });
  await summary.click();
  await expect(page.locator('details[open]')).toContainText('ZIP распакован целиком');
});
test('404, robots и локальный sitemap без вымышленного домена', async ({ page, request }) => {
  expect((await page.goto('/page-that-does-not-exist'))?.status()).toBe(404);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Здесь пока ничего нет.');
  expect((await request.get('/robots.txt')).status()).toBe(200);
  const sitemap = await request.get('/sitemap.xml');
  expect(sitemap.status()).toBe(200);
  expect(await sitemap.text()).not.toContain('<loc>');
});
test('Доступность всех основных страниц (WCAG 2 A/AA)', async ({ page }) => {
  for (const path of pagePaths) {
    await page.goto(path);
    const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
    expect(results.violations, JSON.stringify(results.violations.map(item => ({ id: item.id, nodes: item.nodes.map(node => node.target) })))).toEqual([]);
  }
});
test('200% размер текста и reduced motion', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.addStyleTag({ content: 'html { font-size: 200% !important; }' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).scrollBehavior)).toBe('auto');
});
