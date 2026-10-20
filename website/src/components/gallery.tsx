import Link from 'next/link';
export function Gallery() {
  return <div className="interface-preview" data-testid="interface-preview"><span className="badge">DESKTOP 1.0 · В РАЗРАБОТКЕ</span><h3>Снимки Desktop 1.0 будут добавлены после проверки интерфейса</h3><p>После проверки окон и навигации здесь появятся подписанные изображения реальной сборки. Пока сайт показывает статус разработки. Вымышленные показатели и снимки личного компьютера не публикуются.</p><Link className="text-link" href="/roadmap#desktop-progress">Этапы проверки и ограничения</Link></div>;
}
