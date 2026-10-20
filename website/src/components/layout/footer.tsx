import Link from 'next/link';
import { Github } from 'lucide-react';
import { navigation } from '@/config/site';
import { release } from '@/config/release';
export function Footer() {
  return <footer className="site-footer"><div className="container"><div className="footer-main"><div><Link className="brand" href="/"><span className="brand-mark" aria-hidden="true">N</span>NEXUS</Link><p>Оборудование. Показатели. Ясность.</p></div><nav aria-label="Навигация в подвале">{navigation.map(item => <Link key={item.href} href={item.href}>{item.label}</Link>)}<Link href="/about">О проекте</Link><a href={release.repository}><Github size={16} aria-hidden="true" />GitHub</a></nav></div><div className="footer-bottom"><span>NEXUS Monitor · Alpha {release.version}</span><span>Для Windows x64 · Работает локально</span><a href={release.url}>Материалы релиза</a></div></div></footer>;
}
