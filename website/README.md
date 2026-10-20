# NEXUS Monitor — Website 1.0

Русский сайт NEXUS Monitor: опубликованная Alpha 0.3.1 и разрабатываемая Desktop 1.0. Самостоятельный проект в `website/`, который не требует запущенного Desktop-приложения. Сайт работает локально без backend, базы данных и обращений к GitHub API во время просмотра.

## Запуск

Нужны Node.js 20.9+ и npm. Для разработки рекомендован актуальный Node.js LTS. В этой среде проверен Node.js 24.17.0.

Если Docker-контейнер уже занимает порт 3000, перед запуском сервера разработки выполните `docker compose stop` из папки website.

```powershell
cd website
npm install
npm run dev
```

Откройте **http://localhost:3000**. Сервер слушает только локальный интерфейс 127.0.0.1. Чтобы остановить сервер, нажмите Ctrl+C в его терминале.

## Запуск в Docker

Нужен Docker Desktop в режиме **Linux containers** (либо Docker Engine на Linux) с Docker Compose. Node.js на хосте для запуска контейнера не нужен.

```powershell
cd website
docker compose up --build -d --wait
```

Сайт: **http://localhost:3000**. Compose собирает Next.js в отдельном Node.js 24 этапе и помещает только готовый `out/` в контейнер Nginx 1.30.5 Alpine. Контейнер запускается от пользователя 101:101, с read-only файловой системой и временной `/tmp`, без bind-mount исходников. Все страницы, RSC-файлы навигации, favicon и SEO-файлы входят в образ. Healthcheck проверяет главную страницу.

```powershell
docker compose ps
docker compose logs -f website
docker compose stop
docker compose start
docker compose down
```

`stop` останавливает сайт; `start` запускает уже созданный контейнер; `down` удаляет только ресурсы этого Compose-проекта. После изменения исходников повторите `docker compose up --build -d --wait`. Для первой сборки нужен интернет для официальных базовых образов и npm-зависимостей. Затем образ работает без live GitHub API.

Порт 3000 должен быть свободен. Для одновременного запуска с `npm run dev` используйте другой порт:

```powershell
$env:NEXUS_PORT = '8080'
docker compose up --build -d --wait
```

Тогда сайт откроется на http://localhost:8080. По умолчанию порт опубликован только на 127.0.0.1. Linux shell: `NEXUS_PORT=8080 docker compose up --build -d --wait`.

Для будущего реального домена задайте `SITE_URL` в окружении сборки либо в `.env` рядом с compose.yaml. Compose передаёт его как **build argument**; изменение переменной в уже работающем контейнере не меняет статические metadata. После изменения SITE_URL пересоберите образ. Локально оставьте значение пустым. `.env.local` в Docker-контекст не включается.

Браузерные проверки именно запущенного контейнера (на хосте нужны установленные npm-зависимости и Microsoft Edge):

```powershell
$env:E2E_BASE_URL = 'http://127.0.0.1:3000'
npm run test:e2e
Remove-Item Env:E2E_BASE_URL
```

В этом режиме Playwright не поднимает отдельный Node-сервер: проверяется Nginx из контейнера. Результаты — `test-results-docker/results.json`, HTML-отчёт — `playwright-report-docker/`, снимки — `qa/docker-screenshots/`.

Дополнительно можно проверить HTTP-ответы, 404, редиректы, MIME и кеширование: `node scripts/check-container.mjs http://127.0.0.1:3000`. Отчёт сохраняется в `qa/docker-http-results.json`.

## Команды

| Команда | Действие |
| --- | --- |
| `npm install` | Установка зависимостей по package-lock.json |
| `npm run dev` | Next.js dev на localhost:3000 |
| `npm run lint` | ESLint |
| `npm run typecheck` | Строгая проверка TypeScript |
| `npm run test` | Unit/component-тесты Vitest и Testing Library |
| `npm run build` | Статический production export в out/; postbuild нормализует RSC-пути на Windows |
| `npm run start` | Локальный просмотр готового out/ на localhost:3000 |
| `npm run start -- --port 3001` | Просмотр production export параллельно с dev |
| `npm run test:e2e` | Playwright + axe, production export на отдельном порту 3001 |

Для чистой воспроизводимой установки в дальнейшем можно использовать `npm ci`. Браузерные тесты запускаются после `npm run build`. Сейчас они используют установленный Microsoft Edge (`channel: 'msedge'`); если его нет, установите браузер из официального источника или смените channel в playwright.config.ts и установите Chromium через Playwright. Тестовый сервер поднимается и останавливается автоматически. Порт 3001 должен быть свободен.

## Страницы

| Адрес | Содержание |
| --- | --- |
| `/` | Обзор продукта, разработка Desktop 1.0, 7 возможностей Alpha, интерфейс, производительность и ограничения |
| `/features` | Подробности по компонентам и условия поддержки |
| `/download` | Installer, Portable, архив исходников, размеры, дата, SHA-256, PowerShell, история |
| `/docs` | 19 разделов: 14 сохранённых статей Alpha и 5 отдельных разделов Desktop 1.0 |
| `/roadmap` | 6 этапов Desktop 1.0, ограничения и 4 сохранённых опубликованных релиза |
| `/about` | Назначение и принципы проекта, ссылки на материалы |
| Несуществующий путь | Страница 404 |

Есть error boundary и состояние загрузки. Основные страницы генерируются статически.

## Архитектура и контент

- `src/app/` — App Router, страницы, общие стили и SEO.
- `src/components/` — шапка, подвал, общие блоки, статус изображений, копирование, рендерер документации.
- `src/config/release.ts` — единый типизированный manifest релиза: версия, дата, ссылки, байты, SHA-256, история.
- `src/config/site.ts` — навигация, маршруты и проверка SITE_URL.
- `src/config/screenshots.ts` — пустой манифест будущих проверенных снимков.
- `src/content/docs.ts` — структурированный контент Alpha; `desktop-docs.ts` добавляет 5 отдельных разделов Desktop 1.0. Новая статья добавляется без переписывания навигации и UI.
- `src/lib/` — форматирование и metadata.
- `public/images/PROVENANCE.md` — политика публикации будущих снимков; снимков реального ПК в дереве нет.
- `evidence/` — материалы официального GitHub API и подтверждение URL/контрольных сумм; удаление личных моделей из body явно отмечено в JSON.
- `tests/` — unit/component и E2E; `qa/screenshots/` — реальные снимки сайта.
- `REPORT.md` — архитектура, результаты, ограничения и рекомендации для будущего Fedora-развёртывания.
- `Dockerfile`, `.dockerignore`, `compose.yaml`, `docker/nginx.conf` — сборка и запуск сайта в Docker.

Client Components ограничены шапкой с мобильным меню и кнопкой копирования, а также error boundary. Остальной контент рендерится серверными компонентами на этапе сборки. Tailwind CSS 4 подключён через PostCSS; общая визуальная система описана токенами и компонентными классами в globals.css. Используются Lucide Icons и системный Segoe UI, без внешних шрифтов и аналитических скриптов.

## Обновление релиза

1. Получите подтверждённые материалы нового официального GitHub Release.
2. Обновите release.ts: tag, version, publishedAt, имена файлов, байты, SHA-256 и историю. URL формируются в одном месте.
3. Обновите статьи и ограничения по новой версии; формулировки не должны расширять неподтверждённую поддержку.
4. Сохраните официальный JSON в evidence/ и обновите snapshot-импорт в tests/release.test.ts.
5. Запустите lint, typecheck, unit tests, build и E2E.

Основные страницы не получают данные через live GitHub API. GitHub нужен только при актуализации контента и скачивании приложения пользователем.

## SEO и будущий домен

В локальной версии `SITE_URL` пуст. Canonical и публичные URL не выдумываются, sitemap.xml содержит пустой urlset. У каждой страницы есть уникальные title/description, Open Graph и Twitter metadata. robots.txt и favicon готовы.

Перед отдельной будущей публикацией задайте **реальный origin** в `SITE_URL` при сборке, например через `.env.local` (образец — `.env.example`). Значение должно быть HTTP(S) origin без пути, учётных данных, query и hash. Повторная сборка автоматически добавит canonical, Open Graph URL и шесть страниц в sitemap. Реальный домен сейчас не настроен.

## Изображения и бренд

Снимки реального ПК исключены из публикуемого дерева. Главная и раздел интерфейса честно показывают статус: снимки Desktop 1.0 будут добавлены после отдельной проверки интерфейса и содержимого. Макеты с вымышленными показаниями не используются. Автоматический импорт прежних Alpha-снимков отключён в `scripts/prepare-assets.mjs`.

Цвета фона, карточек, текста и зелёный взяты из src/style.css приложения. Текстовый знак N адаптирован для сайта и не объявлен утверждённым официальным логотипом.

## Техническое замечание Windows

В Next.js 16.4.0 статический экспорт на Windows записывает часть RSC-сегментов как вложенные пути, тогда как браузер запрашивает имена с точками ([issue #92339](https://github.com/vercel/next.js/issues/92339)). scripts/normalize-static-export.mjs в postbuild копирует содержимое в ожидаемые имена. Это проверено unit-тестом и E2E; на Linux уже корректные имена не меняются. Не удаляйте `.txt`-файлы Next.js из out/.

Ничего не опубликовано. Домены, DNS, Fedora, SSH, удалённые репозитории и DEV/PROD не настраивались. Docker подготовлен для локального запуска по отдельному запросу.
