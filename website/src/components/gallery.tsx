import Link from 'next/link';
import Image from 'next/image';
import { screenshots } from '@/config/screenshots';
export function Gallery() {
  return <div className="interface-preview" data-testid="interface-preview">
    <span className="badge">DESKTOP 1.0 · BETA.1 · В РАЗРАБОТКЕ</span>
    {screenshots.map(screenshot => <figure className="desktop-screenshot" key={screenshot.src}>
      <a className="gallery-image" href={screenshot.src} target="_blank" rel="noopener noreferrer" aria-label="Открыть снимок настроек в полном размере">
        <Image src={screenshot.src} width={screenshot.width} height={screenshot.height} alt={screenshot.alt} sizes="(max-width: 767px) 100vw, 1200px" loading="lazy" />
      </a>
      <figcaption><h3>{screenshot.title}</h3><p>{screenshot.description}</p></figcaption>
    </figure>)}
    <Link className="text-link" href="/roadmap#desktop-progress">Этапы проверки и ограничения</Link>
  </div>;
}
