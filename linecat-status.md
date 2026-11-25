# SeasonCat Slint UI status

The Slint UI in `ui/` replaces the Flutter UI in `lib/`, page for page against FlClash. This file lists what each batch
landed and what is left.

## Phase 1: fix what is done but does not match

| Batch | Commit | What |
| --- | --- | --- |
| A logs | 03abeee | Search, level filters (bar and row label) with a keyword strip, ⋯ menu with export and log level, detail sheet with copy, `_LogBody` route rendering, newest first, no clear button, "暂无日志". |
| B dashboard | "dashboard service status, connection stats, intranet IP and editor" (SHA in the next batch's row) | Speed tile figure sized from its character count with 10% slack and shrinking on narrow tiles (fixes the clipped upload figure on device), 18px IP under 400px wide. Service status and connection stats cards, intranet IP card (LAN proxy switch, 无网络), status card not removable (no FAB), IP quality sheet with hide IP, failed probe with tap-to-retry, editor with descriptions, drag to reorder and no reset, wide rows at 208/64/176 (compact 176/56/152) with the summaries row. |

## Remaining

- Batch B leftovers: ServiceFaultsButton and LiveStatusButton in the bar, the 已暂停 (suspended) chip (needs a
  suspend signal from UiBridge), memory detail page with release memory, profile detail sheet with providers and
  切换配置, the service status sheet (order/enable services), the connections table on tall desktop windows.
- Phase 1 batch C (proxies, profiles, requests, developer, UA, navigation, tools, app icons).
- Phase 2: tools settings (theme extras, on-demand), advanced config (rule providers, icon sets, custom proxies, MITM),
  TUN / sniffer / NTP override pages, profile editor and extensions, QR import / TV send / preview, resources leftovers.
- Final page-by-page comparison with FlClash `lib/`.

## Waiting on the owner

- When to delete Flutter, VPN back/close behaviour, release signing, the default log level (kept at info; FlClash
  defaults to error), whether the build workflow skips `slint-ui`, kernel integration, the 「语言」 row, and adding a JS
  engine (rquickjs) for scripts.
