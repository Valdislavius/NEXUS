import { describe, expect, it } from 'vitest';
import { release, releaseHistory } from '@/config/release';
import api from '../evidence/github-release-v0.3.1.json';
import { getSiteUrl } from '@/config/site';
import { formatDate, formatBytes } from '@/lib/format';
import { docs } from '@/content/docs';
import { desktopDevelopment } from '@/config/desktop';
import { screenshots } from '@/config/screenshots';
import { existsSync, readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
describe('Проверенные материалы релиза', () => {
  it('разработка Desktop 1.0 не подменяет опубликованные загрузки Alpha', () => {
    expect(desktopDevelopment.version).toBe('1.0.0-beta.1');
    expect(desktopDevelopment.status).toBe('В разработке');
    expect(release.version).toBe('0.3.1');
    expect(release.assets.installer.url).toContain('/v0.3.1/');
    expect(Object.keys(desktopDevelopment)).not.toContain('assets');
    expect(screenshots).toHaveLength(1);
    expect(screenshots[0]).toMatchObject({ src: '/images/desktop-settings-beta1.jpg', version: '1.0.0-beta.1', capturedOn: '2026-10-23', width: 1282, height: 932 });
    expect(createHash('sha256').update(readFileSync(`public${screenshots[0].src}`)).digest('hex')).toBe(screenshots[0].sha256);
    for (const name of ['overview', 'cpu', 'storage']) expect(existsSync(`public/images/${name}.webp`)).toBe(false);
  });
  it('манифест совпадает с независимым снимком официального GitHub API', () => {
    expect(release.tag).toBe(api.tag_name);
    expect(release.publishedAt).toBe(api.published_at);
    for (const asset of Object.values(release.assets)) {
      const official = api.assets.find(item => item.name === asset.name);
      expect(official).toBeDefined();
      expect(asset.bytes).toBe(official?.size);
      expect(asset.url).toBe(official?.browser_download_url);
      expect(asset.sha256.toLowerCase()).toBe(official?.digest.replace('sha256:', '').toLowerCase());
    }
  });
  it('история и документация не теряют текущий релиз и уникальные якоря', () => {
    expect(releaseHistory[0].version).toBe(release.version);
    expect(docs).toHaveLength(19);
    for (const id of ['desktop-overview','desktop-testing-export','desktop-overlay','desktop-performance','desktop-compatibility']) expect(docs.find(section => section.id === id)).toBeDefined();
    expect(new Set(docs.map(section => section.id)).size).toBe(docs.length);
    expect(docs.find(section => section.id === 'limitations')).toBeDefined();
  });
});
describe('Origin сайта и форматирование', () => {
  it('не придумывает origin для локальной версии', () => { expect(getSiteUrl('')).toBeUndefined(); });
  it('нормализует подтверждённый origin и отклоняет небезопасные значения', () => {
    expect(getSiteUrl('https://example.org/')).toBe('https://example.org');
    for (const url of ['javascript:alert(1)', 'https://user:password@example.org', 'https://example.org/path', 'https://example.org?token=x']) expect(() => getSiteUrl(url)).toThrow();
  });
  it('показывает московскую дату и двоичный размер без ложной точности', () => {
    expect(formatDate('2026-10-07T22:30:00Z')).toContain('8 октября 2026');
    expect(formatBytes(1048576)).toBe('1 МиБ');
  });
});
