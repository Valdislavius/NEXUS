import Link from 'next/link';
import { BookOpen } from 'lucide-react';
import { docs } from '@/content/docs';
import { release } from '@/config/release';
import { PageIntro } from '@/components/ui';
import { DocumentationBlock } from '@/components/docs/block';
import { pageMetadata } from '@/lib/metadata';
export const metadata = pageMetadata('Документация', 'Руководство Alpha 0.3.1 и отдельная документация Desktop 1.0: мониторинг, история, диагностика, тесты, экспорт, трей, оверлей, ресурсы и совместимость.', '/docs');
export default function DocsPage() {
  return <div className="container docs-layout"><aside className="docs-nav"><div className="docs-nav-title"><BookOpen size={16} aria-hidden="true" />ДОКУМЕНТАЦИЯ</div><nav aria-label="Разделы документации">{docs.map((section, index) => <Link key={section.id} href={`#${section.id}`}><span aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>{section.title}</Link>)}</nav></aside><article className="docs-article"><PageIntro eyebrow={`РУКОВОДСТВО · ALPHA ${release.version}`} title="Разобраться. Установить. Наблюдать.">Как начать работу с NEXUS и что делать, если показание недоступно. Сначала — руководство опубликованной Alpha {release.version}; далее — отдельные разделы разрабатываемой Desktop 1.0.</PageIntro>{docs.map((section, index) => <section className="docs-section" key={section.id} id={section.id} aria-labelledby={`${section.id}-title`}><h2 id={`${section.id}-title`}>{section.title}</h2>{section.blocks.map((block, blockIndex) => <DocumentationBlock key={blockIndex} block={block} />)}<nav className="docs-pager" aria-label={`Переход из раздела ${section.title}`}>{index > 0 && <Link className="text-link" href={`#${docs[index - 1].id}`}>Предыдущий: {docs[index - 1].title}</Link>}{index < docs.length - 1 && <Link className="text-link" href={`#${docs[index + 1].id}`}>Далее: {docs[index + 1].title}</Link>}</nav></section>)}</article></div>;
}
