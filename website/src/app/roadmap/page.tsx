import Link from 'next/link';
import { release, releaseHistory } from '@/config/release';
import { desktopDevelopment } from '@/config/desktop';
import { formatDate } from '@/lib/format';
import { PageIntro } from '@/components/ui';
import { pageMetadata } from '@/lib/metadata';
export const metadata = pageMetadata('Roadmap — развитие проекта', 'NEXUS Desktop 1.0: локальная разработка, ограничения и этапы проверки. Отдельно — опубликованная Alpha 0.3.1 и история релизов.', '/roadmap');
const stages = [
  ['01', 'Архитектура и данные', 'Разделение провайдеров, статусы и свежесть метрик, безопасные источники оборудования, замеры нагрузки.'],
  ['02', 'Интерфейс и мониторинг', 'Боковая навигация, обзор системы, страницы оборудования, единая система графиков.'],
  ['03', 'История и диагностика', 'SQLite, ограниченные запросы за период, агрегация, Timeline и правила предупреждений.'],
  ['04', 'Тесты и отчёты', 'Ручные безопасные проверки с отменой, история результатов, экспорт Excel, CSV и PDF.'],
  ['05', 'Фоновая работа', 'Трей, автозапуск без окна, настраиваемые сочетания клавиш и поддерживаемый оконный оверлей.'],
  ['06', 'Проверка и выпуск', 'Регрессии, installer/portable, ресурсные бюджеты, воспроизводимые игровые измерения и аппаратная совместимость.'],
] as const;
export default function RoadmapPage() {
  return <div className="container page-content">
    <PageIntro eyebrow="РАЗВИТИЕ NEXUS" title="Движение к более полной картине.">Опубликованная Alpha и разработка Desktop 1.0 — разные этапы. Здесь можно увидеть их границы и порядок проверки.</PageIntro>
    <section id="desktop-progress" className="development-panel" aria-labelledby="development-title"><span className="badge badge-green">{desktopDevelopment.label} · {desktopDevelopment.status}</span><h2 id="development-title">Локальная ветка {desktopDevelopment.version}</h2><p>Новая версия объединяет мониторинг с аналитикой и диагностикой. Окончательный статус готовности зависит от сборок, автоматизированных тестов и реальной проверки Windows. Публичный релиз 1.0 и сроки пока не объявлены.</p><div className="plans-grid">{stages.map(([number, title, text]) => <article className="plan-card" key={number}><span className="technical-label">ЭТАП {number}</span><h3>{title}</h3><p>{text}</p></article>)}</div><div className="notice"><h3>Отдельные ограничения 1.0</h3><p>GPU/VRAM-тесты, надёжный источник FPS, эксклюзивный полноэкранный режим и неподтверждённые датчики требуют отдельных решений и испытаний. Они не выдаются за готовые универсальные возможности. Телеметрия без безопасного источника остаётся недоступной.</p><Link className="text-link" href="/docs#desktop-compatibility">Совместимость и ограничения</Link></div></section>
    <section className="subsection"><h2>Опубликованные версии</h2><p>Текущая загрузка — Alpha {release.version}. История предыдущих релизов сохранена.</p><div className="roadmap-list" aria-label="Опубликованные релизы">{releaseHistory.map(item => <article className="roadmap-item" key={item.version}><div><span className={`badge ${item.version === release.version ? 'badge-green' : ''}`}>ALPHA {item.version}</span><time dateTime={item.date}>{formatDate(item.date)}</time></div><div><span className="technical-label">ОПУБЛИКОВАНА</span><h2>{item.title}</h2><p>{item.description}</p><a className="text-link" href={`${release.repository}/releases/tag/v${item.version}`}>Материалы релиза v{item.version}</a></div></article>)}</div></section>
    <div className="notice subsection"><h3>Разработка открыта для проверки</h3><p>Проверенные загрузки, ограничения и опубликованные результаты находятся в официальном GitHub проекта. Локальная версия сайта не публикует приложение или новую версию автоматически.</p><a className="text-link" href={release.repository}>Открыть GitHub NEXUS</a></div>
  </div>;
}
