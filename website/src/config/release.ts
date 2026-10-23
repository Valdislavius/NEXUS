export interface ReleaseAsset {
  name: string;
  label: string;
  url: string;
  bytes: number;
  sha256: string;
}
const repository = 'https://github.com/Valdislavius/NEXUS';
const tag = 'v0.3.1';
const asset = (name: string, label: string, bytes: number, sha256: string): ReleaseAsset => ({
  name, label, bytes, sha256, url: `${repository}/releases/download/${tag}/${name}`,
});
export const release = {
  name: 'NEXUS Monitor', version: '0.3.1', stage: 'Alpha', platform: 'Windows x64',
  tag, repository, publishedAt: '2026-10-08T12:03:23Z',
  url: `${repository}/releases/tag/${tag}`,
  checksumsUrl: `${repository}/releases/download/${tag}/SHA256-0.3.1.txt`,
  assets: {
    installer: asset('NEXUS-Setup.exe', 'Установщик', 218827969, '1A5E6D30869BCDAED1C90973B9A5C3F902E66090CD453AF83B26D6EC3875423A'),
    portable: asset('NEXUS-Desktop-0.3.1-portable-windows-x64.zip', 'Portable', 4480407, 'B4412A3E590DFC7CDC4C072BF7A962D6720F240A099442DB3A2F3305C0607A93'),
    source: asset('NEXUS-Desktop-0.3.1-source.zip', 'Исходный код', 1624777, '5785E1EDDFDB8623440BEDD9A24C27992E45EA465683F98E8BFB67A85CE7A724'),
  },
} as const;
export const releaseHistory = [
  { version: '0.3.1', date: '2026-10-08', title: 'Доступ к датчикам. Больше ясности.', description: 'Проверка прав администратора, подробные SATA SMART, причины недоступности датчиков и экспорт обезличенной диагностики.' },
  { version: '0.3.0', date: '2026-09-20', title: 'Температура Intel и единая установка.', description: 'CPU Package Temperature через PawnIO на поддерживаемых Intel CPU. Единый установщик с офлайн-установщиком WebView2.' },
  { version: '0.2.0', date: '2026-09-20', title: 'Новый взгляд на оборудование.', description: 'Обновлённые карточки, крупные показатели, подробности CPU, RAM и накопителей. Сброс устаревших значений при потере источника.' },
  { version: '0.1.0', date: '2026-09-20', title: 'Первый публичный Alpha.', description: 'Локальный мониторинг CPU, GPU, RAM, накопителей, системы и сети. Интервалы обновления 2, 5 и 10 секунд.' },
] as const;
