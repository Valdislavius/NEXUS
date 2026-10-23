import Link from 'next/link';
export default function NotFound() {
  return <div className="container error-page"><div className="eyebrow">404</div><h1>Здесь пока ничего нет.</h1><p>Страница не найдена. Возможно, адрес изменился или в нём есть опечатка.</p><div className="hero-actions"><Link className="button button-primary" href="/">На главную</Link><Link className="button button-secondary" href="/docs">Документация</Link></div></div>;
}
