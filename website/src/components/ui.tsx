import Link from 'next/link';
import { Download } from 'lucide-react';
import type { ReactNode } from 'react';
import { release } from '@/config/release';
export function DownloadButton({ children = 'Скачать для Windows', className = '' }: { children?: ReactNode; className?: string }) {
  return <a className={`button button-primary ${className}`} href={release.assets.installer.url}><Download size={18} aria-hidden="true" />{children}</a>;
}
export function PageIntro({ eyebrow, title, children }: { eyebrow: string; title: string; children: ReactNode }) {
  return <div className="page-intro"><div className="eyebrow">{eyebrow}</div><h1>{title}</h1><div className="lead">{children}</div></div>;
}
export function CallToAction() {
  return <section className="cta-panel" aria-labelledby="cta-title"><div><span className="eyebrow">NEXUS MONITOR · ALPHA {release.version}</span><h2 id="cta-title">Познакомьтесь с вашим ПК.</h2><p>Установите NEXUS и посмотрите, как работает ваше оборудование.</p></div><div className="cta-actions"><DownloadButton /><Link className="text-link" href="/download">Все варианты загрузки</Link></div></section>;
}
