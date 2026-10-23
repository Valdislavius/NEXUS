'use client';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';
import { Menu, X, Download, Github } from 'lucide-react';
import { navigation } from '@/config/site';
import { release } from '@/config/release';
export function Header() {
  const pathname = usePathname().replace(/\/+$/, '') || '/';
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') { setOpen(false); toggle.current?.focus(); } };
    const onClick = (event: PointerEvent) => { if (!menu.current?.contains(event.target as Node) && !toggle.current?.contains(event.target as Node)) setOpen(false); };
    const media = window.matchMedia('(min-width: 1024px)');
    const onResize = () => { if (media.matches) setOpen(false); };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onClick);
    media.addEventListener('change', onResize);
    return () => { document.removeEventListener('keydown', onKey); document.removeEventListener('pointerdown', onClick); media.removeEventListener('change', onResize); };
  }, [open]);
  return <header className="site-header"><div className="container header-inner">
    <Link href="/" className="brand" aria-label="NEXUS Monitor — главная" onClick={() => setOpen(false)}><span className="brand-mark" aria-hidden="true">N</span><span>NEXUS<span className="brand-sub">MONITOR</span></span></Link>
    <nav className="desktop-nav" aria-label="Основная навигация">{navigation.map(item => <Link key={item.href} href={item.href} aria-current={pathname === item.href ? 'page' : undefined}>{item.label}</Link>)}<a href={release.repository} className="github-nav"><Github size={16} aria-hidden="true" />GitHub</a></nav>
    <div className="header-actions"><Link href="/download" className="button button-small button-primary"><Download size={16} aria-hidden="true" />Скачать</Link><button ref={toggle} className="icon-button menu-toggle" type="button" aria-label={open ? 'Закрыть меню' : 'Открыть меню'} aria-expanded={open} aria-controls="mobile-menu" onClick={() => setOpen(!open)}>{open ? <X /> : <Menu />}</button></div>
    <div ref={menu} id="mobile-menu" className="mobile-menu" hidden={!open}><nav aria-label="Мобильная навигация">{navigation.map(item => <Link key={item.href} href={item.href} aria-current={pathname === item.href ? 'page' : undefined} onClick={() => setOpen(false)}>{item.label}</Link>)}<a href={release.repository}>GitHub</a></nav></div>
  </div></header>;
}
