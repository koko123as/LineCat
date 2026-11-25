# SeasonCat Slint UI status

The Slint UI in `ui/` replaces the Flutter UI in `lib/`, page for page against FlClash. This file lists what each batch
landed and what is left.

## Phase 1: fix what is done but does not match

| Batch | Commit | What |
| --- | --- | --- |
| A logs | 03abeee | Search, level filters (bar and row label) with a keyword strip, ⋯ menu with export and log level, detail sheet with copy, `_LogBody` route rendering, newest first, no clear button, "暂无日志". |
| B dashboard | b0c3b35 | Speed tile figure sized from its character count with 10% slack and shrinking on narrow tiles (fixes the clipped upload figure on device), 18px IP under 400px wide. Service status and connection stats cards, intranet IP card (LAN proxy switch, 无网络), status card not removable (no FAB), IP quality sheet with hide IP, failed probe with tap-to-retry, editor with descriptions, drag to reorder and no reset, wide rows at 208/64/176 (compact 176/56/152) with the summaries row. |
| B2 dashboard | "dashboard faults badge, memory and profile sheets" | ServiceFaultsButton (engine.state faults, badge in the bar, alert listing service and reason), memory detail sheet with release memory (`core.releaseMemory`, banner with what was freed), profile detail sheet (counts from `profile.inspect` and `provider.list`, subscription, last update, providers), 切换配置 swap button and picker sheet. |

## Remaining

- Batch B leftovers: LiveStatusButton and its deck (needs config warnings, update download and MITM consent first),
  the 已暂停 chip (FlClash's suspend is "current SSID in the excluded SSIDs", so it lands with the on-demand page and
  its Wi-Fi SSID bridge method), the service status sheet (order/enable services), the connections table on tall
  desktop windows, the 运行时配置 preview button in the profile sheet (with profile preview), the 扩展脚本 value
  (with profile extensions).
- Phase 1 batch C (proxies, profiles, requests, developer, UA, navigation, tools, app icons).
- Phase 2: tools settings (theme extras, on-demand), advanced config (rule providers, icon sets, custom proxies, MITM),
  TUN / sniffer / NTP override pages, profile editor and extensions, QR import / TV send / preview, resources leftovers.
- Final page-by-page comparison with FlClash `lib/`.

## Waiting on the owner

- When to delete Flutter, VPN back/close behaviour, release signing, the default log level (kept at info; FlClash
  defaults to error), whether the build workflow skips `slint-ui`, kernel integration, the 「语言」 row, and adding a JS
  engine (rquickjs) for scripts.
