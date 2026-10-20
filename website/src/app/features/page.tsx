import { Check } from 'lucide-react';
import Link from 'next/link';
import { features } from '@/components/features/catalog';
import { CallToAction, PageIntro } from '@/components/ui';
import { pageMetadata } from '@/lib/metadata';
import { DesktopDevelopment } from '@/components/desktop-development';
export const metadata = pageMetadata('Возможности', 'Что показывает NEXUS Monitor: загрузка CPU, NVIDIA NVML, память, SATA SMART, NVMe Health, сеть и локальная диагностика. Поддержка и ограничения Alpha 0.3.1.', '/features');
export default function FeaturesPage() {
  return <div className="container"><PageIntro eyebrow="ВОЗМОЖНОСТИ NEXUS" title="Каждый компонент. В деталях.">Текущие показатели и аппаратные сведения помогают понять, как работает ваш компьютер. Ниже — возможности и границы поддержки Alpha 0.3.1.</PageIntro><nav className="component-nav" aria-label="Компоненты">{features.map(item => <Link key={item.id} className="button button-small button-secondary" href={`#${item.id}`}>{item.name}</Link>)}</nav><div className="page-content">{features.map(({ icon: Icon, ...item }) => <section className="detail-feature" id={item.id} key={item.id}><div className="detail-feature-heading"><div className="feature-icon"><Icon size={25} aria-hidden="true" /></div><div><span className="technical-label">{item.label}</span><h2>{item.name}</h2></div></div><div><p>{item.description}</p><ul className="check-list">{item.details.map(detail => <li key={detail}><Check size={17} aria-hidden="true" />{detail}</li>)}</ul><div className="notice"><h3>Условия и ограничения</h3><p>{item.limitation}</p></div><Link className="text-link" href={`/docs#${item.id === 'cpu' ? 'cpu-temperature' : item.id === 'gpu' ? 'gpu' : item.id === 'storage' ? 'sata' : item.id === 'diagnostics' ? 'diagnostics' : 'overview'}`}>Читать в документации</Link></div></section>)}</div><DesktopDevelopment /><CallToAction /></div>;
}
