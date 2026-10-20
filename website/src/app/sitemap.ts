import type { MetadataRoute } from 'next';
import { pagePaths, siteUrl } from '@/config/site';
export const dynamic = 'force-static';
export default function sitemap(): MetadataRoute.Sitemap {
  return siteUrl ? pagePaths.map(path => ({ url: `${siteUrl}${path}`, changeFrequency: 'monthly', priority: path === '/' ? 1 : 0.7 })) : [];
}
