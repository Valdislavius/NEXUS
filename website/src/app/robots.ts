import type { MetadataRoute } from 'next';
import { siteUrl } from '@/config/site';
export const dynamic = 'force-static';
export default function robots(): MetadataRoute.Robots {
  return { rules: { userAgent: '*', allow: '/' }, ...(siteUrl ? { sitemap: `${siteUrl}/sitemap.xml` } : {}) };
}
