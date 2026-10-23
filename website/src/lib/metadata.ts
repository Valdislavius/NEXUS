import type { Metadata } from 'next';
import { siteUrl } from '@/config/site';
export function pageMetadata(title: string, description: string, path: string): Metadata {
  return {
    title, description,
    ...(siteUrl ? { alternates: { canonical: `${siteUrl}${path}` } } : {}),
    openGraph: { title: `${title} — NEXUS Monitor`, description, locale: 'ru_RU', type: 'website', ...(siteUrl ? { url: `${siteUrl}${path}` } : {}) },
    twitter: { card: 'summary', title: `${title} — NEXUS Monitor`, description },
  };
}
