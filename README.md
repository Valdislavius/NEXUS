# NEXUS Desktop

Мониторинг оборудования для Windows x64: CPU, GPU, RAM, накопители, плата, система и сеть.

## Скачать

**[Alpha 0.3.1 — актуальная версия](https://github.com/Valdislavius/NEXUS/releases/tag/v0.3.1)**

- [NEXUS-Setup.exe — единый установщик](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/NEXUS-Setup.exe)
- [Portable Windows x64](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/NEXUS-Desktop-0.3.1-portable-windows-x64.zip)
- [Полный исходный код 0.3.1](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/NEXUS-Desktop-0.3.1-source.zip)
- [Контрольные суммы SHA-256](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/SHA256-0.3.1.txt)

Установщик включает приложение, Collector, лицензии и официальный WebView2 Offline Installer. Установка остаётся для текущего пользователя; интернет для установки и мониторинга не требуется. Перед обновлением закройте NEXUS. Portable распаковывается целиком и требует установленного WebView2 Runtime.

## Alpha 0.3.1

- Штатный автоматический UAC, проверка эффективных прав приложения и наследование прав Collector.
- Intel CPU Package через установленный PawnIO и подписанный IntelMSR; TjMax берётся из процессора. Отображение температуры подтверждено пользователем на i5-12600KF после установки 0.3.1.
- Расширенный SATA SMART: отдельные RAW, нормализованные значения и доступные пороги. На Samsung SSD 860 EVO 1TB получены 14 атрибутов через Windows Storage; NVMe SMART сохранён.
- Понятные причины «Нет данных», очистка устаревших показаний и локальный обезличенный экспорт диагностики. Дизайн и существующие функции сохранены.
- Go-тесты/vet, 28 frontend-тестов и 5 Rust-тестов прошли.

PawnIO устанавливается отдельно; приложение не устанавливает драйвер скрытно. AMD пока не получает температуру: исследованный SMN-транспорт требует записи selector-регистра, запрещённой текущей политикой. Другие модели CPU/накопителей, повышенный native ATA и чистая Windows без Runtime физически не подтверждены. Повышение всего приложения — временное решение Alpha.

## Документация и проверки

- [REPORT.md](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/REPORT.md)
- [Совместимость Intel/AMD](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/CPU-COMPATIBILITY-0.3.1.md)
- [SATA SMART: атрибуты, единицы и ограничения](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/SATA-SMART-0.3.1.md)
- [Результаты тестирования](https://github.com/Valdislavius/NEXUS/releases/download/v0.3.1/TEST-RESULTS-0.3.1.json)

Репозиторий служит архивом релизов. Полный проект 0.3.1 опубликован отдельным **NEXUS-Desktop-0.3.1-source.zip** в Assets. Автоматические архивы GitHub **Source code** содержат файлы репозитория; для сборки приложения используйте отдельный полный архив. Лицензии и соответствующие материалы сторонних компонентов включены в дистрибутивы и исходники.

## Предыдущие версии

- [Alpha 0.3.0](https://github.com/Valdislavius/NEXUS/releases/tag/v0.3.0)
- [Alpha 0.2.0](https://github.com/Valdislavius/NEXUS/releases/tag/v0.2.0)
- [Alpha 0.1.0](https://github.com/Valdislavius/NEXUS/releases/tag/v0.1.0)

Все версии — предварительные альфа-сборки. Доступность показаний зависит от оборудования и драйверов; SMART не гарантирует исправность накопителя.
