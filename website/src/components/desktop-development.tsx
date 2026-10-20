import Link from 'next/link';
import { desktopDevelopment } from '@/config/desktop';
import { release } from '@/config/release';

export function DesktopDevelopment() {
  return <section className="desktop-development section" id="desktop-1" aria-labelledby="desktop-1-title">
    <div className="section-heading"><div>
      <div className="eyebrow">СЛЕДУЮЩАЯ ГЛАВА · DESKTOP 1.0</div>
      <h2 id="desktop-1-title">От показаний — к пониманию системы.</h2>
      <p>Мониторинг, история, диагностика и проверка ПК в одном локальном центре.</p>
    </div><span className="badge badge-green">{desktopDevelopment.status}</span></div>
    <div className="notice development-notice"><h3>Два разных этапа продукта</h3><p>Для загрузки опубликована Alpha {release.version}. Desktop {desktopDevelopment.version} разрабатывается в отдельной ветке и проходит проверку. Перечисленные ниже направления относятся к 1.0 и не объявляются возможностями опубликованной Alpha.</p><Link className="text-link" href="/roadmap#desktop-progress">Состояние разработки и ограничения</Link></div>
    <div className="plans-grid">{desktopDevelopment.features.map((feature, index) => <article className="plan-card" key={feature.title}><span className="technical-label">DESKTOP 1.0 · {String(index+1).padStart(2, '0')}</span><h3>{feature.title}</h3><p>{feature.text}</p></article>)}</div>
  </section>;
}
