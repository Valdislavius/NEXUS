# NEXUS Alpha 0.3.1 — Hardware Sensors Update

Дата: 8 октября 2026 года. Проект 0.3.0 продолжен; Tauri 2, React/TypeScript, Go, дизайн и контракт метрик version=1 сохранены.

Это исторический отчёт Alpha 0.3.1, обезличенный для импорта исходников в Git. Конкретные модели тестового компьютера и накопителей, фактические температуры, SMART-счётчики, наработка и другие индивидуальные показания удалены. Таблицы поддерживаемых семейств и определения протокола являются справочными сведениями, а не описанием компьютера пользователя. Опубликованный релиз Alpha и его архивы не изменяются. Описанная ниже архитектура UAC относится к исторической Alpha; актуальная Desktop-архитектура описана отдельно.

## Результат

Release 0.3.1 включает автоматическое штатное повышение прав приложения, сохранённый Intel CPU Package через PawnIO, отдельный AMD адаптер с достоверным отказом для недопустимого транспорта, расширенный SATA SMART и безопасный экспорт диагностики. Текущие CPU/GPU/RAM/диски/плата/ОС/сеть, фильтры, анимации, интервалы и повторное обнаружение сохранены.

Артефакты находятся в существующей структуре release:

- installer/NEXUS-Setup.exe — единый NSIS EXE с офлайн-установщиком WebView2.
- portable/NEXUS.exe и nexus-collector.exe — комплект для Windows x64.
- NEXUS-Desktop-0.3.1-portable-windows-x64.zip — portable ZIP.
- NEXUS-Desktop-0.3.1-source.zip — исходники, тесты, scripts, lock-файлы, документация и лицензии.
- SHA256-0.3.1.txt — контрольные суммы конечных артефактов.
- TEST-RESULTS-0.3.1.json и тематические test/hardware logs — итоговые результаты проверок; наличие неподтверждённых физических веток указано явно.

## UAC, Tauri и установка

В release EXE встроен application manifest requestedExecutionLevel=requireAdministrator, uiAccess=false. Он подключён штатным tauri-build::WindowsAttributes::app_manifest; зависимость Common Controls v6 сохранена. До создания mutex, WebView и Collector release проверяет эффективный TOKEN_ELEVATION. Если обычный launcher оставил неповышенный token, NEXUS один раз вызывает штатный ShellExecuteW runas для собственного current_exe с фиксированным retry flag и завершает первоначальный процесс. Retry без elevated token и ошибка проверки безопасно завершают запуск; параметры внешних процессов не принимаются. Tauri создаёт Collector обычным дочерним процессом, который наследует проверенный повышенный token. Никакой runas для сторонних компонентов, конфигурации UAC, ACL, службы или тихой установки драйвера не добавлено. При отказе от UAC UI и Collector не запускаются; обхода нет. На проверенном host manifest дал прямой ERROR_ELEVATION_REQUIRED740, но обычный PowerShell Shell open первоначально вернул TokenElevation=0. Явный RunAs подтвердил TokenElevation=1 / TokenElevationTypeFull=2 у обоих процессов. Причина поведения launcher не обобщается на Windows; для этого случая добавлена проверка effective token и единственный штатный повтор. Уже повышенный процесс и политика Windows могут изменить наличие приглашения. [Microsoft: application manifest](https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests).

Manifest приложения независим от manifest NSIS: установка остаётся currentUser, RequestExecutionLevel user, данные установки HKCU, прежние productName/identifier сохранены для обновления 0.3.0. Поддержка запуска EXE requireAdministrator после установки проверена по исходникам установленного CLI 2.11.4 и его закреплённого nsis_tauri_utils v0.5.3: RunAsUser использует ShellExecuteW для стандартного UAC, включая fallback ERROR_ELEVATION_REQUIRED. Перед обновлением/удалением рекомендуется закрыть NEXUS; автоматическое закрытие повышенного приложения из неповышенного установщика зависит от прав и сеанса. [Tauri: currentUser и WebView2](https://v2.tauri.app/distribute/windows-installer/), [исходники RunAsUser](https://github.com/tauri-apps/nsis-tauri-utils/blob/nsis_tauri_utils-v0.5.3/crates/nsis-process/src/lib.rs).

Установщик включает официальный Microsoft WebView2 Standalone Offline Installer и Collector; ручное копирование не требуется. Ветка отсутствующего Runtime не требует загрузки из сети. Portable требует установленный Runtime; используйте единый Setup, если его нет. PawnIO не входит в дистрибутив как устанавливаемый драйвер. Удаление NEXUS не удаляет общие WebView2/PawnIO. Debug/Cargo test используют asInvoker, чтобы не повышать среду разработки; распространяемая release сборка требует UAC.

## Температуры CPU

Intel сохраняет CPUID GenuineIntel, package thermal capability leaf6 EAX bit6, реальные IA32_TEMPERATURE_TARGET 0x1A2 и IA32_PACKAGE_THERM_STATUS 0x1B1, validity bit31 и target−delta. Оба чтения происходят на одном закреплённом потоке; affinity восстанавливается. TjMax не фиксирован. Не используются core/ACPI substitute, температура по нагрузке или произвольный MSR. Signed IntelMSR.bin не изменён: D6ED85D65AB17A22F813EF98207D6D537155EE2DED5976A21CB48413C9B92E5F. Происхождение, LGPL и соответствующие исходники сохранены в docs/PawnIO.

Handles переиспользуются при успешном чтении, закрываются при ошибке, redetection и завершении. Недостоверное полное показание очищает значение и повторяет только чтение. Ошибочные reopen ограничены пятью попытками и интервалом 30 секунд; новое обнаружение сбрасывает бюджет. Причины включают отсутствие PawnIO, права, неподдерживаемость, ответ драйвера и временную ошибку.

AMD определяется CPUID до любого Intel доступа и использует отдельный адаптер. Исследованы AMD OSRR 56255, LibreHardwareMonitor Amd17Cpu, PawnIO.Modules 0.2.11 AMDFamily17. SMN 0x59800 является реальным источником Tctl, но исследованный read_smn сначала записывает PCI selector 0x60. Пользователь запретил запись аппаратных регистров; этот путь не вызывается, новый AMD module/driver не добавлен. Учтены различия family/model и модельных Tctl/Tdie offsets в исследовании. Без подтверждённого транспорта вычисление температуры не выполняется.

| CPU | Результат / достоверность |
| --- | --- |
| Тестовый Intel CPU с архитектурным package sensor и PawnIO | На тестовой системе ранее подтверждено отображение температуры с правами администратора. Неповышенное чтение дало WIN32_5 и null. В установленной Alpha подтверждены elevated tokens приложения и Collector. Конкретная модель CPU и фактические температурные показания в эту обезличенную копию не включены. |
| Другие Intel с архитектурным package sensor | Условная совместимость после корректного реального ответа; физически другие модели не проверены. |
| Intel без package sensor / несколько пакетов / >64 logical CPU | Явный unsupported; значения не подменяются. |
| AMD family17h/19h/1Ah | unsupported / AMD_SMN_SELECTOR_WRITE_REQUIRED; безопасная запись-запрещённая ветка, физическая температура не заявлена. |
| Другие AMD / не amd64 | Явный unsupported без предположений. |

Подробная таблица, документация Intel/AMD, техническое обоснование и уровни проверки: CPU-COMPATIBILITY-0.3.1.md.

## SATA SMART

Используются закрытый ATA DATA_IN command allowlist IDENTIFY DEVICE EC, SMART READ DATA B0/D0 и необязательное READ THRESHOLDS B0/D1; native ATA_PASS_THROUGH_EX соответствует ULONG_PTR Windows. Нужный API read/write handle не означает наличие write-команд. Один сектор, таймаут 3 секунды, никаких включений SMART, self-test, записи журналов или произвольных taskfile. Проверяются размер, смещения, ATA status, checksum, revision и уникальность ID.

Резервный путь читает уже существующий IOCTL_STORAGE_PREDICT_FAILURE VendorSpecific[512] только для ATA/SATA и валидирует тот же SMART sector. STORAGE_DEVICE_DESCRIPTOR используется только для модели и шины; serial не декодируется. Windows Storage composite temperature имеет приоритет; NVMe SMART сохранён. USB/RAID/SCSI не получают непроверенного ATA passthrough.

| Атрибут / смысл | Ограниченная семантика |
| --- | --- |
| 09 / 0C: наработка / включения | Документированные Samsung, ограниченные Crucial/Kingston; неизвестные модели RAW |
| 05: переназначения | Samsung секторы; Crucial NAND блоки; WD HDD отдельный профиль |
| C5 / C6 / BB: pending / uncorrectable | Только соответствующий vendor профиль; Micron ECC не называется HDD сектором |
| BE / C2: температура | Модельный формат; прежний Windows sensor сохраняется |
| F1 / F6: запись | Samsung F1 ×512 bytes; Micron/WD LBA без предположения единицы bytes |
| B1 / AD / CA / E7: износ / ресурс | RAW NAND cycles и normalized раздельно; ресурс только по документированной шкале |
| Остальные возвращённые ID | Current/Worst/Flags, точный RAW48 decimal+HEX; Threshold только при корректном чтении |

На одном тестовом SATA-накопителе через Windows Storage подтверждено чтение таблицы SMART и температуры; на тестовом NVMe сохранён прежний SMART-путь. Конкретные модели, численные показания, наработка, объёмы записи и счётчики ошибок исключены из публикуемой копии. Эти наблюдения не являются гарантией текущего состояния накопителей. Native ATA открытие неповышенным процессом вернуло WIN32_5; повышенный native path физически не подтверждён. Дополнительные профили Crucial, Kingston и WD HDD проверены искусственными данными, не физически.

Полная таблица профилей и атрибутов, источники Microsoft/производителей и единицы: SATA-SMART-0.3.1.md. Без результата пороги остаются недоступны; RAW неизвестного устройства не становится процентом здоровья. Обнулённый счётчик допустим только при реальном ответе.

## Диагностика и интерфейс

Структурированное поле metric.diagnostic добавлено совместимо с version=1: unsupported, permission_denied, dependency_missing, device_unresponsive, driver_no_data, invalid_response, transient_error. CPU, ATA/storage и NVML дают технические коды, а UI показывает короткую причину. NVML классификация соответствует официальному nvmlReturn_t, отсутствующая библиотека/функция отделена от ошибок GPU. Неполученные поля никогда не заменяются нулём.

Расширения UI ограничены температурной причиной, таблицей SATA и настройками диагностики. RAW, normalized Current/Worst и Threshold представлены отдельно. После sensor failure прежняя metric становится null, RAW-таблица заменяется текущей; при stale/общем IPC failure UI скрывает весь старый SMART и численные показания.

Безопасный диагностический режим показывает текущие статусы, не открывает дополнительные devices и не меняет расписание опроса. Экспорт вызывает один Rust command без параметров и создаёт локальный JSON по allowlist. Не включаются host, user, IP/MAC, serial, PNP ID, device model/name, OEM specs, полный snapshot/warnings/reasons или произвольный driver error. Содержатся generic component ordinal, sensor type, фиксированный source, status, bounded code, timestamp, collector state и до128 записей переходов ошибок. Frontend повторно валидирует контракт перед Blob download. Автоматической передачи или сохранения истории нет.

## IPC и жизненный цикл

Один Collector на экземпляр; session-local mutex препятствует второму экземпляру новой версии. Restart сериализован, старый Collector kill/wait завершается до нового, generation подавляет запоздалые events. Выход завершает процесс и ждёт его; EOF с родителя отслеживается до инициализации Windows providers. Полная очередь advisory interval commands не блокирует EOF/stop. Повреждённое JSON Lines сообщение, неверная version/type, frame >2MiB или ошибка pipe останавливает Collector и выводит ошибку без сохранения старого значения как текущего. Diagnostics export учитывает процесс, error и возраст snapshot.

Старая 0.3.0 не знает mutex новой версии; её необходимо закрыть перед параллельным portable запуском 0.3.1. WMI/DeviceIoControl могут задержать опрос; UI отражает stale. Отдельных HTTP серверов, сетевых зависимостей мониторинга и повышенных сторонних процессов нет.

## Тестирование и границы подтверждения

Итоговые машинные результаты находятся в release/TEST-RESULTS-0.3.1.json и artifacts/alpha031. Последние проверки выполняются над конечными исходниками/дистрибутивом; временные ограничения sandbox не объявляются аппаратным отказом.

| Проверка | Объём |
| --- | --- |
| Go unit tests, go vet | CPU decode/CPUID/handles/retry/null; ATA headers/status/checksum/vendor units/stale/handles; NVMe; NVML categories; core lifecycle; EOF при заполненной очереди |
| Frontend | 28 тестов: schema, missing reason, RAW/normalized, stale table, diagnostics privacy/export, прежние card/actions |
| Rust | 5 passing tests: privacy allowlist, capped transitions log, stopped statuses, effective-token decisions и bounded UAC retry; test harness без повышения |
| Production/Tauri/NSIS | Release build, embedded manifests, currentUser, offline Microsoft-signed WebView2, Collector pair и архивы |
| Реальные sensors | SATA fallback + NVMe; Intel access denied; пользовательское прежнее Intel admin подтверждение отдельно |
| Неподтверждённое | AMD physical, другие Intel/vendors, повышенный native ATA, clean offline VM, полный DPI/load matrix |
| UI/UAC | GUI automation была остановлена тайм-аутом доступа Computer Use. Пользователь подтвердил температуру финальной 0.3.1 после UAC; обычный запуск дал elevated tokens обоим процессам, один Collector. Отмена UAC проверена логикой/fixtures; физическое нажатие «Нет» не выполнялось. |

Обновление 0.3.0 → 0.3.1 прошло: exit0, совпадающий Collector. У установленного EXE ожидаемо отличается только Tauri bundle tag UNK→NSS относительно portable. Обычный запуск окончательной сборки подтвердил elevated application/Collector и ровно один дочерний процесс. Пользователь подтвердил температуру Intel. Потеря родителя завершила Collector; обычное закрытие окна пользователем также оставило оба процесса завершёнными. В новом каталоге установка/удаление прошли exit0, оба EXE и HKCU uninstall entry исчезли. На исходном каталоге после первого удаления оставались файлы прежней 0.3.0; они не удалялись вручную. После тестов окончательная 0.3.1 возвращена в обычный каталог пользователя и оставлена запущенной. Сохранённый установщиком путь тестового каталога при финальной установке явно заменён обычным расположением; настройки UAC/ACL/служб не менялись. Clean Windows/ветка отсутствующего Runtime и физическая отмена UAC не проверены. Машинные результаты: release/TEST-RESULTS-0.3.1.json.

## Риски и известные ограничения

1. Всё release приложение, WebView host и Collector работают повышенными после UAC. Это временная архитектура Alpha; расширяет последствия дефекта UI/IPC/provider. CSP и фиксированные команды сохранены, внешние ссылки/команды из UI не добавлены.
2. CurrentUser каталог и portable находятся в месте, доступном пользователю на запись. Подмена EXE/sidecar локальным вредоносным процессом остаётся риском при последующем согласии UAC; криптографическая привязка sidecar не заявляется. EXE не подписан сертификатом издателя; возможен SmartScreen.
3. Standard user может потребовать credentials администратора; при другом admin account пользовательский профиль/WebView2 environment может отличаться. UAC не отключается; UI может вообще не стартовать при отказе.
4. PawnIO нужен отдельно для Intel и должен давать доступ; UAC не гарантирует аппаратную поддержку CPU.
5. AMD безопасно остаётся без температуры при запрещённом selector-write. Максимальная подтверждённая поддержка в этой политике — корректный vendor/family detection и объяснимый отказ.
6. ATA данные/пороги зависят от controller/firmware. Windows fallback может отдавать данные, кэшированные driver. USB/RAID не заявлены. RAW48 расширения не предполагаются.
7. На NVMe ресурс означает used endurance, на SATA отдельные vendor counters; ни один показатель не гарантирует исправность или сохранность данных.
8. Clean Windows без Runtime/PawnIO, другие GPU/сокеты, больше64 logical CPU, длительная нагрузка и весь DPI диапазон не проверены в этой сессии. Runtime/provider installation не выполнялась скрыто.

## Рекомендации для следующей версии

Выделить минимальный повышенный sensor broker с аутентифицированным локальным IPC и неизменяемым allowlist, оставив UI asInvoker. Защитить/подписать дистрибутив и привязать Collector к доверенной сборке. Для AMD искать документированный vendor/provider read-only API и проверять Tctl/Tdie offsets на физических поколениях; расширение политики selector-write требует отдельного технического решения. Провести физические native ATA/USB/controller/Crucial/Kingston/WD испытания с моделями и firmware. Добавлять семантические профили только с документацией и физическими образцами. Проверить clean offline install/update/uninstall, отказ UAC, alternative admin account, долгий load и DPI125/150/200%.
