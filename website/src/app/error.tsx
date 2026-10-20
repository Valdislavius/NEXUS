'use client';
import Link from 'next/link';
export default function ErrorPage({ reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return <div className="container error-page"><div className="eyebrow">ОШИБКА ЗАГРУЗКИ</div><h1>Не удалось открыть страницу.</h1><p>Попробуйте загрузить её ещё раз или вернитесь на главную.</p><div className="hero-actions"><button className="button button-primary" onClick={reset}>Повторить</button><Link className="button button-secondary" href="/">На главную</Link></div></div>;
}
