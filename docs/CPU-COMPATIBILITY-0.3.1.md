# CPU temperature compatibility — NEXUS Alpha 0.3.1

Дата аудита: 8 октября 2026 года. Поддержка определяется CPUID и реальным ответом датчика, а не торговым названием CPU.

## Совместимость

| CPU / конфигурация | Источник | Статус Alpha 0.3.1 | Уровень проверки |
| --- | --- | --- | --- |
| Проверенный Intel CPU, один физический пакет, Windows x64 | Подписанный IntelMSR / установленный PawnIO; MSR 0x1A2 и 0x1B1 | Реализация сохранена; требуются права на PawnIO | Пользователь ранее подтвердил отображение температуры с правами администратора; модель и числовые показания исключены из публичного отчёта. При неповышенном тесте подтверждён `WIN32_5`, `null`. В конечной 0.3.1 после обычного запуска проверены elevated tokens приложения и Collector; пользователь подтвердил отображение температуры 08.10.2026. Число нового показания не сообщалось. Декодирование отдельно проверено синтетической имитацией регистров. |
| Другие Intel с CPUID leaf 6 EAX bit 6, корректным IA32_TEMPERATURE_TARGET и package readout | Та же отдельная Intel-реализация | Условная совместимость: только после успешного реального ответа; нет фиксированного TjMax | Формат архитектурных регистров и контроль доступа проверены; другие физические модели в этой сессии не тестировались. |
| Intel без package thermal management | Нет разрешённого package-источника | `unsupported / INTEL_PACKAGE_SENSOR_ABSENT`, `null` | Проверено имитированным CPUID. Core и ACPI не подставляются вместо Package. |
| AMD Family 17h (Zen / Zen+ / Zen 2, в зависимости от модели) | Исследован SMN / SMU thermal block | `unsupported / AMD_SMN_SELECTOR_WRITE_REQUIRED`, `null` | Отдельный AMD-адаптер и выбор производителя проверены имитацией. Физический AMD отсутствует; сборщик не открывает PawnIO для AMD. |
| AMD Family 19h (включает поколения Zen 3 / Zen 4) | Исследован PawnIO AMDFamily17 / LibreHardwareMonitor | Тот же явный неподдерживаемый статус | Запрет записи selector-регистра делает изученный транспорт неприменимым. Физическая проверка не выполнялась. |
| AMD Family 1Ah (включает Zen 5) | Исследован PawnIO AMDFamily17 | Тот же явный неподдерживаемый статус | Диапазон family принят из исходников модуля; это не заявление о подтверждённой температуре на всех моделях. |
| Остальные / будущие AMD family | Нет проверенного безопасного транспорта | `unsupported / AMD_FAMILY_UNVERIFIED`, `null` | Без эвристики по нагрузке, Intel MSR и предполагаемого смещения. |
| Несколько физических CPU | Отсутствует безопасное сопоставление package | `unsupported / CPU_MULTIPLE_PACKAGES_UNSUPPORTED`, `null` | Имитация проверяет закрытие прежнего handle и очистку числа. |
| Более 64 логических CPU / processor groups | Требует отдельного group-aware сопоставления | `unsupported / CPU_PROCESSOR_GROUPS_UNSUPPORTED`, `null` | Реальная такая система не тестировалась. |
| Windows не amd64 | Отдельный безопасный отказ | `unsupported / CPU_ARCHITECTURE_UNSUPPORTED`, `null` | Hardware transport в этой сборке только Windows x64. |

Торговые серии AMD пересекаются по поколениям, особенно у мобильных APU. Семейство/модель декодируется из CPUID leaf 1 с extended family/model; таблица не обещает поддержку температуры на основе номера Ryzen.

## Почему AMD пока возвращает «Нет данных»

AMD OSRR 56255 Rev 3.03, §4.2.1 описывает `SMU::THM::THM_TCON_CUR_TMP` по SMN `0x00059800`: текущее control-показание в bits 31:21 и выбор диапазона bit 19. §5–6 также описывает APML / SB-TSI и независимые температурные offset-регистры. Эти сведения подтверждают реальный источник, но сами по себе не дают разрешённого Windows-транспорта к нему. [Официальная документация AMD, PDF](https://www.amd.com/content/dam/amd/en/documents/processor-tech-docs/programmer-references/56255_OSRR.pdf).

Изученный официальный PawnIO.Modules 0.2.11 `AMDFamily17.p` экспортирует `ioctl_read_smn`, однако внутри сначала вызывает `pci_config_write_dword(..., SMN_INDEX_OFFSET, offset)`, затем читает `SMN_DATA_OFFSET`. Selector находится по PCI offset `0x60`, data — `0x64`. Поэтому вызов с названием read технически записывает аппаратный регистр. Пользователь явно запретил такие записи; NEXUS не загружает AMD-модуль и не вызывает этот IOCTL. Модуль покрывает family 17h–1Ah, имеет LGPL-2.1-or-later, но происхождение и лицензия не устраняют этот конфликт. [PawnIO AMDFamily17, зафиксированный tag 0.2.11](https://github.com/namazso/PawnIO.Modules/blob/0.2.11/AMDFamily17.p).

LibreHardwareMonitor применяет этот транспорт к Tctl/Tdie и отдельным CCD. Его AMD-реализация учитывает шаг 0.125 °C, range correction −49 °C, отдельные модельные offset: −20 °C для 1600X/1700X/1800X, −10 °C для 2700X и −27 °C для определённых Threadripper. Адреса CCD отличаются у поколений; это ещё одна причина не выдавать общую guessed-temperature или универсальный offset. Такой декодер без разрешённого реального чтения не делает поддержку рабочей, поэтому в NEXUS не добавлена недоступная ветка с фиктивным результатом. [LibreHardwareMonitor Amd17Cpu.cs](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitorLib/Hardware/Cpu/Amd17Cpu.cs), [его PawnIO-адаптер](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitorLib/PawnIo/AmdFamily17.cs).

SB-TSI требует подходящего доступа к SMBus контроллеру/платформе; проверенного универсального Windows API, доступного текущему сборщику без записей аппаратных регистров, в ходе этого аудита не установлено. Транспорт Ryzen Master SDK, SMU mailbox и произвольное MMIO тоже не добавлены: в этой задаче отсутствует подтверждённый путь, удовлетворяющий полному запрету записей и проверке привилегированных зависимостей. Это ограничение изученных путей, а не утверждение, что безопасного источника никогда не может существовать.

## Сохранённое декодирование Intel

Перед открытием драйвера проверяются `GenuineIntel`, наличие CPUID leaf 6 и package thermal management EAX bit 6. Два MSR читаются на одном привязанном Windows-потоке; исходная affinity восстанавливается. Используется действительный target из bits 23:16 `0x1A2` и delta из bits 22:16 `0x1B1`. Проверка validity bit 31 сохранена из работающей 0.3.0. Результат: `target − delta`; неподходящий target (вне прежнего защитного диапазона 50–125 °C), отсутствующая validity и отрицательный результат дают `null`. Программирование TCC, thermal interrupts и любые MSR writes отсутствуют. [Intel SDM: package thermal management и CPUID](https://cdrdv2-public.intel.com/835755/253669-sdm-vol-3b.pdf), [Intel: пример target/delta и package readout](https://community.intel.com/t5/Software-Archive/Intel-Xeon-Phi-Reading-MSR/m-p/1170935).

Набор разрешённых NEXUS MSR остаётся строго `{0x1A2, 0x1B1}`, включая проверку полного 64-bit адреса до обращения к драйверу. Signed IntelMSR.bin не заменён; SHA256 `D6ED85D65AB17A22F813EF98207D6D537155EE2DED5976A21CB48413C9B92E5F`. Его существующее происхождение, исходники, лицензия и инструкции пересборки перечислены в `docs/PawnIO/NOTICE.md` и соответствующих архивных исходниках. Модуль имеет дополнительные upstream handlers, но NEXUS использует только `ioctl_read_msr` и собственный более узкий allowlist.

## Handles, повторное обнаружение и диагностика

Успешный handle переиспользуется; ошибочный load немедленно закрывает только что открытый handle. Ошибка MSR чтения закрывает текущий handle. Повторное обнаружение и завершение сборщика закрывают прежний adapter. Ошибка полного, но недостоверного показания очищает metric и повторяет только чтение на следующем опросе без лишнего reopen.

После ошибки открытия / загрузки / чтения новый open разрешён не чаще одного раза за 30 секунд и максимум пять попыток подряд до явного повторного обнаружения. Успешное чтение сбрасывает бюджет, позволяя восстановиться после последующего временного сбоя. Сборщик не крутит бесконечный цикл открытия устройства; UI не удерживает прежнее число как свежее.

Диагностика различает `unsupported`, `permission_denied`, `dependency_missing`, `device_unresponsive`, `driver_no_data`, `invalid_response`, `transient_error`. В JSON передаются понятная фиксированная причина и ограниченный символический код либо `WIN32_<номер>`. Текст syscall исключён из пользовательской причины; внутренний error сохраняет cause для разработчика. Ни серийные номера, ни имена пользователя, ни пути или IP в CPU-диагностику не включаются. Отсутствующий sensor всегда публикуется с `value: null`.

## Проверки этого изменения

- Прошли 13 целевых CPU unit-тестов: декодирование реального target и invalid bits; allowlist; CPUID family/model; отдельный AMD dispatch; переиспользование/закрытие handles; ошибки open/load/target/status; cooldown и восстановление; конечный бюджет reopen; очистка свежего значения до `null`; смена производителя; ошибки affinity; категории ошибок; длина MSR ответа; несколько packages.
- Hardware smoke без повышения прав выполнил реальную попытку чтения на текущей Intel-системе: `permission_denied / WIN32_5`, `celsius: null`. Это подтверждение отказа в доступе, а не подтверждение температуры этой сборки с UAC.
- `go vet ./...` прошёл. Общие Go-тесты и проверка повышенного релизного запуска фиксируются в основном `REPORT.md`; аппаратные WMI/дисковые проверки в sandbox имеют отдельные ограничения доступа.
- Физический AMD и другие Intel модели в этой сессии не проверялись. Автоматические проверки на fixtures не заменяют аппаратные.

## Следующая версия

Выделить повышенный sensor broker с фиксированным IPC allowlist вместо повышенного UI. Для AMD рассмотреть документированный vendor/provider API, который отдаёт идентифицированное, свежее Tctl/Tdie без прямых register writes со стороны NEXUS; проверить происхождение, подпись, лицензию и реальный комплект зависимостей. Если в будущем будет отдельно разрешён selector-write для чтения, его необходимо явно оформить как изменение политики, синхронизировать PCI bus access, ограничить адреса по family/model, различать Tctl/Tdie/CCD и проверить offset на соответствующих физических CPU. До этого AMD остаётся явно неподдерживаемым датчиком.

## Проверка эффективных прав перед запуском

Основной механизм — release manifest `requireAdministrator`. Его наличие подтверждено в release EXE: прямой CreateProcess вернул `ERROR_ELEVATION_REQUIRED / 740`. В конкретной проверенной среде обычный PowerShell Start-Process / Shell open всё же запустил приложение и Collector с `TokenElevation=0`; явный RunAs дал `TokenElevation=1` и `TokenElevationTypeFull=2` обоим. Причина поведения этого launcher не установлена и не обобщается на Windows.

Поэтому `src-tauri/src/elevation.rs` дополнительно проверяет собственный эффективный token через `OpenProcessToken(TOKEN_QUERY)` и `GetTokenInformation(TokenElevation)` до mutex, WebView и Collector. Повышенный token разрешает продолжить. Неповышенный первый старт вызывает штатный `ShellExecuteW` с verb `runas` только для собственного абсолютного `current_exe` и единственного фиксированного аргумента `--nexus-uac-retry`; исходный процесс заканчивается до создания Collector. Пользовательские аргументы и сторонние executable не передаются. Отказ UAC прекращает старт. Повторный неповышенный запуск или ошибка проверки token безопасно завершаются с фиксированным понятным сообщением. Запросы не зацикливаются; token handle и COM initialization закрываются штатно. [Microsoft: TokenElevation](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_elevation), [GetTokenInformation](https://learn.microsoft.com/en-us/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation), [ShellExecuteW / runas и UAC](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew).

Guard включён только в release Windows; debug не запрашивает дополнительные права, неподдерживаемый non-Windows release не пытается имитировать UAC. Два чистых Rust-теста проверяют решения для повышенного/неповышенного/повторного запуска и ошибки token query, а также точное совпадение `OsStr` retry flag. Общий `cargo test --manifest-path src-tauri/Cargo.toml --locked --offline` после интеграции guard прошёл: 5 тестов, 0 ошибок. Обычные debug-тесты не компилируют native release ветку: её подтверждение относится к последующей release сборке и проверке фактического token.

Эти проверки подтверждают механизм прав и отказов. Они не подтверждают текущее числовое показание Intel после UAC в GUI: оно должно быть отдельно зафиксировано на физическом устройстве. Прежнее пользовательское подтверждение отображения температуры относится к исходной работающей реализации, а текущий CPU smoke в sandbox — к реальному `WIN32_5 / null`. Повышенный UI остаётся временным архитектурным решением Alpha и требует отдельного broker в следующей версии.

## Финальный запуск 0.3.1

Release проверяет TOKEN_ELEVATION до UI/Collector. Manifest остаётся основным способом UAC; если launcher сохранил неповышенный token, допускается единственный штатный ShellExecuteW runas только собственного EXE. Query failure, отмена или неуспешный retry завершают запуск. Обычный запуск конечного установленного EXE подтвердил повышенные права обоих процессов и ровно один Collector. Пользователь подтвердил, что температура CPU отображается; точное новое значение не записано. Физическое AMD/native ATA подтверждение этим тестом не устанавливается.
