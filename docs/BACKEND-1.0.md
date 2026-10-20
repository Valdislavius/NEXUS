# Native services: NEXUS Desktop 1.0 Preview

This document describes implemented code, not a declaration that Desktop 1.0 has passed release acceptance. The source version is `1.0.0-beta.1`; the published Alpha 0.3.1 release remains a separate historical release.

## Process and privilege model

`src-tauri/src/main.rs` owns one Collector child, its bounded JSON-line protocol reader, and native services. The existing Go hardware providers remain authoritative. The wire version remains 1. New services do not issue MSR, PCI, SMN or physical-disk writes.

The executable manifest now uses `asInvoker` in both development and release builds. It no longer relaunches the entire UI through UAC. Drivers and sensors that require privileges can therefore return permission-denied/unavailable results. There is currently no privileged Windows service or automatic elevated broker. Running as administrator is an explicit user choice, not an application startup requirement. The previous UAC decision tests remain in `elevation.rs` as regression tests, but that module is compiled only in tests and is not invoked by the application.

The session-local named Windows mutex prevents concurrent NEXUS instances and duplicate Collectors. An unexpected Collector exit closes its pipe, terminates/reaps its child, marks live data unavailable and permits a manual restart. Protocol messages have a 2 MiB framing limit.

## Module boundaries

| Module | Responsibility |
|---|---|
| Go `collector/` | Hardware inventory, sensor providers, normalization and explicit availability |
| `main.rs` | Collector lifecycle, IPC commands and orchestration |
| `storage.rs` | SQLite binding, batched history, aggregation, Timeline and saved results |
| `diagnostics_engine.rs` | Threshold/health/telemetry rules over existing readings |
| `diagnostics.rs` | Preserved allowlisted sensor-availability JSON report |
| `benchmark.rs` | Manual, cancellable and bounded user-mode checks |
| `report.rs` | Streaming XLSX/CSV and paginated PDF summary |
| `settings.rs` | Validated preferences, atomic persistence, current-user autostart |
| `desktop.rs` | Native tray, main WebView lifetime, hotkeys and Win32 GDI overlay |
| `windows_checks.rs` | Bounded read-only Windows WHEA/PnP and traceroute checks |

## History, correctness and resource limits

History is stored in the current user's application-data directory, normally `%APPDATA%\local.nexus.desktop\nexus-history.sqlite`. Preferences are adjacent in `desktop-settings.json`; generated reports are under `reports/`. The local database is not an anonymized report: component IDs and provider names can identify hardware. It is excluded from Git.

The database uses the Windows-provided `winsqlite3.dll`; no database service or downloaded native library is required. Windows 10/11 x64 is the supported target. The Rust wrapper uses prepared statements, bound values, finalized statements, FULLMUTEX connections and an outer service mutex. WAL mode, NORMAL synchronization, a 2 MiB SQLite page cache and a 250 ms busy timeout are configured.

Actual `Metric.at` timestamps are retained. Repeated cached readings are deduplicated by sensor timestamp/component/metric. Unavailable values remain SQL NULL, never zero or interpolated values. Provider/unit changes produce separate graph series and separate aggregate statistics. Schema version 2 migrates the initial preview aggregate table without deleting its data; a database from a newer unsupported schema is rejected rather than downgraded.

The background write buffer normally flushes after 15 seconds or 4,096 pending rows. Explicit chart/report queries and orderly exit flush pending readings. Its hard maximum is 8,192 rows. Under sustained storage errors the oldest 4,096 queued rows are dropped to keep memory bounded; `diagnostics_current.historyDroppedSamples` and `storageError` expose the loss. Failed writes retain a bounded retry buffer, and failed transactions attempt rollback. No silent claim of a complete history is made.

Hourly maintenance retains approximately 24 hours of raw readings, then rolls older data into one-minute `min/max/sum/count` aggregates. Weighted averages use sums/counts rather than averaging averages. Aggregates and Timeline events retain a configurable 1–90 days, default seven days. At most 500 benchmark results are kept; history APIs return at most 100 results. Existing aggregate intervals cannot reconstruct sub-minute detail at a period boundary. No continuous VACUUM runs in the background; SQLite reuses freed pages.

`history_query` accepts periods up to 90 days, at most 32 metric IDs, and 16–1,200 rendered points per series (default 300). SQL performs the grouping; the application never loads the full raw period into memory. At most 32 series and 40,000 grouped rows are processed per request. Timeline responses have an absolute 1,000-event limit.

At startup, an invalid/malformed database detected while opening is renamed with a `corrupt-<timestamp>` suffix, together with existing WAL/SHM sidecars, before a new database is created. The recovery is reported. This is preservation and restart, not repair of lost rows or a substitute for backups. The tests exercise corrupt input preservation, reopen/persistence, duplicate suppression, bound parameters, rollups and bounded retry behavior.

## Diagnostics and Timeline

Rules consume already collected metrics; they do not initiate additional sensor polls or stress tests. Implemented rules cover configurable CPU/GPU/storage temperature thresholds, RAM pressure, nonzero supported SMART warnings/error counters, abrupt temperature changes and changes in the reported device inventory.

Each finding includes the component, observed value/unit, detection timestamp, rule reason, possible explanations and suggested actions. Indirect observations do not certify an equipment fault or throttling. A missing/stale source cannot resolve a previous warning: the unresolved finding remains marked `telemetryUnavailable`, and the overall state becomes insufficient data when appropriate. Recovery events require an available reading from the same sensor. Events are persisted on transitions to avoid writing the same warning at every poll.

Diagnostic thresholds configure warnings only. Benchmark protective limits are separately fixed; increasing a user warning threshold cannot disable test protection.

## Manual checks and limits

Checks start only through `benchmark_start`; normal diagnosis and autostart never start them. One test may run at a time. A request duration must be 1–30 seconds, default five. The worker and supervisor are separate from the UI; cancellation is checked in short slices. CPU worker failures return a failed result instead of unwinding the main process.

| Kind | Implemented operation and interpretation |
|---|---|
| `cpu_single` | One integer-algorithm worker, approximately 50% duty cycle, operations/s |
| `cpu_multi` | Up to 16 workers, approximately 50% duty cycle, same algorithm; compare only equivalent configurations |
| `memory` | 64 MiB of process-owned memory, write/read pattern with an optimizer barrier and observable checksum; MiB/s includes checking and pauses |
| `storage_read` | Read at most 64 MiB from the executable file; Windows file cache and deliberate pauses affect throughput |
| `network_dns` | One system `nslookup` of example.com, with a five-second process timeout |
| `network_ping` | Four native documented ICMP requests to a fixed public endpoint, paced one second apart, 750 ms timeout per request; loss and min/avg/max latency |
| `network_traceroute` | Fixed public endpoint, numeric/no-DNS route, at most eight hops, 500 ms per hop wait and hard 15-second process timeout |
| `motherboard` | Collected motherboard availability plus up to 20 WHEA events from the last 24 hours and present System-class PnP error counts; partial/unavailable results are explicit |

The read-file check is **not a benchmark of a selected physical drive**. The RAM check does not test all DIMMs or measure true hardware latency. CPU results are algorithm-specific, do not match third-party benchmark scales and do not assert stability under sustained stress. There are no disk write tests, VRAM tests or synthetic GPU workloads in this preview. GPU/VRAM testing needs a compatible documented compute/graphics implementation and validation on multiple vendors.

Load-generating checks refuse to start when a fresh available temperature is already at the protective limit: 100 °C for CPU/GPU or 80 °C for storage. During a test, two distinct fresh sensor readings at a protective limit stop its own workload. Cached repetitions do not count as new confirmation. Unavailable temperatures cannot provide thermal protection; short duration, bounded allocation and duty limits remain enforced. These limits are conservative application policy, not a statement about each manufacturer's operating specifications.

Network checks are manual, one at a time and share a 15-second cooldown independent of intervening non-network tests. No background ping, DNS test, traceroute or speed test is scheduled. DNS timeout/ICMP filtering do not establish a network adapter fault. Result descriptions omit target/response IPs so they can be included in an anonymized report. Traceroute/WHEA/PnP subprocess output is drained with a 64 KiB capture limit; commands have fixed paths and bounded timeouts, cancellation and no user-supplied shell fragments. WHEA/PnP counts review registered Windows evidence, not the physical health of every controller.

## Reports

`report_generate` takes a format, period, optional component IDs and anonymization flag (default true). Reports are created under application data and the saved absolute path is returned to the UI. Partial output is removed on generation failure.

* CSV is a UTF-8 BOM flat metric table with UTC timestamp, component, kind, metric, unit, provider, min/max/average/count. String cells escape quotes and spreadsheet-formula prefixes.
* XLSX is streamed as standards-based OOXML/ZIP with a 64 KiB output buffer and incremental CRC. It contains Summary, Metrics, Diagnostics, Tests and component summary sheets. Numeric cells remain numeric; missing readings remain empty. Summary includes an actual bounded CPU-load chart when that metric is selected and present. Units are never combined on the chart. The metric table is capped at 1,000,000 rows, with an explicit request to shorten the period when exceeded; ZIP64 is not implemented.
* PDF is a paginated English diagnostic summary with a green header/footer, period, per-metric min/max/average/count, a bounded event summary and benchmark results. It is intentionally less detailed than XLSX. It uses a standard font and mapped ASCII units; localized explanations are in XLSX. Unicode PDF typography/embedded fonts and full metric-history plots are not implemented.

Anonymized exports use generated component aliases and generic provider descriptions. They never serialize the full snapshot, inventory specs, host name, serial number, local IP/MAC/DNS/gateway or provider error text. Component selections apply to metrics and Timeline queries; benchmark results are filtered by compatible component kind because the basic checks are global process-level operations, not per-device tests.

## Native background desktop

The Tauri configuration creates no WebView at process initialization. A normal launch creates the main window on demand; `--background` starts Collector, storage, tray and native event loop without a main WebView. Enabling autostart writes only `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\NEXUS Desktop`, with the absolute executable path and `--background`. This is reversible and does not install a service or scheduled task.

With background mode and close-to-tray enabled, closing the main window destroys the WebView rather than hiding an actively rendering React tree. The native tray can recreate it; a double-click opens NEXUS. Its menu opens diagnostics/settings, toggles the overlay and explicitly exits. A real native lifecycle test must verify the resulting WebView2 child-process behavior after Windows has released its processes; it cannot be established from code inspection alone.

The mini/advanced overlay is a separate Win32 topmost, no-activate, click-through layered window, painted with GDI. It does not create a second WebView, inject a DLL, hook a game or interact with anti-cheat software. Selected readings are updated only when the overlay is shown; no overlay redraw timer runs while hidden. While visible, a one-second timer marks stale numeric values unavailable. Advanced mode draws at most 64 recent samples per selected metric. History on those small sparklines is contextual; the full history page carries timestamps and events.

Position, scale, opacity and metrics are configurable. The overlay supports ordinary windowed/borderless desktop composition. Exclusive fullscreen, protected surfaces, every game/anti-cheat combination and FPS/frame-time collection are not supported or claimed. The overlay does not request a faster sensor interval by itself; it follows the selected monitoring profile, preserving background cost.

Hotkeys use documented `RegisterHotKey` with no-repeat. Defaults are Ctrl+Shift+F10 (overlay) and Ctrl+Shift+F9 (main window). Empty strings disable a binding. A collision causes a clear error and rollback to the previous configuration; settings are atomically persisted only after registration succeeds. A native message thread blocks when idle.

Sleep/wake behavior relies on the Collector's actual timestamps/rate counters and explicit stale states. Automated forced sleep/resume and hardware hot-plug acceptance remain separate physical-machine tests. The application does not change power configuration.

## IPC contract

JSON fields use camelCase; timestamps ending in `Ms` are Unix epoch milliseconds.

* `current_snapshot`, `set_interval`, `restart_collector`, `export_diagnostics`: preserved Alpha commands.
* `history_query({query:{componentId?,metricIds?,fromMs,toMs,maxPoints?}})`: series with componentId, metricId, unit, source, bounded points and min/max/avg.
* `timeline_query({query:{fromMs?,toMs?,limit?}})`: persistent events with timestamp/source/component/severity/title/details/data.
* `diagnostics_current()`: state, findings, freshness, availability counts and storage error/drop information.
* `benchmark_start({request:{kind,durationSeconds?}})`, `benchmark_status()`, `benchmark_cancel()`, `benchmark_history()`.
* `report_generate({request:{format,fromMs?,toMs?,components?,anonymized?}})`: path, format, rows, anonymized.
* `desktop_settings()`, `desktop_settings_save({settings})`, `overlay_toggle()`, `desktop_show({page?})`.

Events: `nexus:snapshot`, `nexus:error`, `nexus:benchmark`, `nexus:navigate`. Snapshot events are emitted only while a main WebView exists. Recreated windows receive their initial navigation through the location hash/session storage.

## Verification

Run the repository's normal frontend/Collector tests and `cargo test --manifest-path src-tauri/Cargo.toml`. The `--self-check <absolute-directory>` mode produces a clearly labeled **synthetic integration fixture**, SQLite history and anonymized CSV/XLSX/PDF files without creating a WebView or launching hardware workloads. Validate those files with an independent OOXML/ZIP/PDF reader. Synthetic fixtures are not evidence that sensors or Windows lifecycle features passed a hardware test.

CPU/RAM budgets and game FPS/1% low/frame-time impact require measured before/after tests on the same Windows system. Neither this architecture nor passing unit tests proves the requested 1% CPU/100 MiB/2–3% FPS goals. Record the application version, profile, window/overlay state, duration, process tree and hardware for each measurement.
