import type { Metadata } from 'next';
import { Header } from '@/components/layout/header';
import { Footer } from '@/components/layout/footer';
import { siteUrl } from '@/config/site';
import './globals.css';
export const metadata: Metadata = {
  title: { default: 'NEXUS Monitor — мониторинг оборудования', template: '%s — NEXUS Monitor' },
  description: 'Локальный мониторинг CPU, NVIDIA GPU, RAM, накопителей и сети для Windows x64. NEXUS Monitor Alpha 0.3.1.',
  ...(siteUrl ? { metadataBase: new URL(siteUrl) } : {}),
  icons: { icon: '/favicon.svg' },
};
export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="ru"><body><a className="skip-link" href="#main">Перейти к содержимому</a><Header /><main id="main" tabIndex={-1}>{children}</main><Footer /></body></html>;
}
