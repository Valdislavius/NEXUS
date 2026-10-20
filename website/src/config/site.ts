export const navigation = [
  { href: '/features', label: 'Возможности' },
  { href: '/download', label: 'Загрузки' },
  { href: '/docs', label: 'Документация' },
  { href: '/roadmap', label: 'Roadmap' },
] as const;
export const pagePaths = ['/', '/features', '/download', '/docs', '/roadmap', '/about'] as const;
export function getSiteUrl(value = process.env.SITE_URL): string | undefined {
  if (!value?.trim()) return undefined;
  const parsed = new URL(value);
  if (!['https:', 'http:'].includes(parsed.protocol) || parsed.username || parsed.password || parsed.search || parsed.hash || parsed.pathname !== '/') {
    throw new Error('SITE_URL must be a public HTTP(S) origin without a path, credentials, query or hash.');
  }
  return parsed.origin;
}
export const siteUrl = getSiteUrl();
