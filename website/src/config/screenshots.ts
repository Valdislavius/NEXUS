export interface Screenshot {
  src: string; title: string; description: string; alt: string;
  width: number; height: number; version: string; capturedOn: string; sha256: string;
}
// Unmodified capture of the real portable build, reviewed for private details.
export const screenshots: Screenshot[] = [{
  src: '/images/desktop-settings-beta1.jpg',
  title: 'Desktop 1.0 beta.1 — настройки',
  description: 'Реальная сборка 1.0.0-beta.1, 23 октября 2026: профили мониторинга, история, фоновый режим и горячие клавиши. Снимок настроек не подтверждает проверку всех аппаратных и игровых сценариев.',
  alt: 'Окно настроек NEXUS: профили сбора, срок хранения истории, поведение при закрытии и сочетания клавиш.',
  width: 1282, height: 932, version: '1.0.0-beta.1', capturedOn: '2026-10-23',
  sha256: '5bf6ca47dd73bfb378325f7d3273dd48ddb6dd69f34e4236ad227be335976492',
}];
