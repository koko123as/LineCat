# SeasonCat Slint UI status

The Slint UI in `ui/` replaces the Flutter UI in `lib/`, page for page against FlClash. This file lists what each batch
landed and what is left.

## Phase 1: fix what is done but does not match

| Batch | Commit | What |
| --- | --- | --- |
| A logs | 03abeee | Search, level filters (bar and row label) with a keyword strip, ⋯ menu with export and log level, detail sheet with copy, `_LogBody` route rendering, newest first, no clear button, "暂无日志". |
| B dashboard | b0c3b35 | Speed tile figure sized from its character count with 10% slack and shrinking on narrow tiles (fixes the clipped upload figure on device), 18px IP under 400px wide. Service status and connection stats cards, intranet IP card (LAN proxy switch, 无网络), status card not removable (no FAB), IP quality sheet with hide IP, failed probe with tap-to-retry, editor with descriptions, drag to reorder and no reset, wide rows at 208/64/176 (compact 176/56/152) with the summaries row. |
| B2 dashboard | "dashboard faults badge, memory and profile sheets" | ServiceFaultsButton (engine.state faults, badge in the bar, alert listing service and reason), memory detail sheet with release memory (`core.releaseMemory`, banner with what was freed), profile detail sheet (counts from `profile.inspect` and `provider.list`, subscription, last update, providers), 切换配置 swap button and picker sheet. |
| C1 batch C part 1 | "connection detail menu, navigation rules and small fixes" | Connection detail ⋯ per `trackerDetailMenuItems` (复制主机 / 复制目标地址 / 复制摘要 with `trackerSummaryText`), developer 消息测试 sends one banner per level, 主题 row uses a paintbrush icon, 代理 leaves the bar and rail when the Core has no groups (before the first load: when there are no profiles), a window narrowed on 请求 shows 仪表盘 and widening returns to 请求 (`shownPageLabelOf`). |
| Fix device taps | "dashboard taps, cat colour, floating dock and proxies menu" | Status card taps restored: b0c3b35 left the editor's drag TouchArea above every card and enabled outside edit mode, swallowing all taps (cat, country icon, IP). It is now enabled only while editing and a 400ms long press lifts the card (follows the finger, swaps past 60% of a neighbour, saves on swap). Cat strokes use the theme primary while running. Floating dock (approved departure): radius 32, 12px from the sides and bottom, pages pad by `Dock.pad`. Proxies ⋯ is FlClash's popup menu (风格 / 排序 / 布局 / 尺寸 / 图标样式 / 隐藏超时代理), layout and size shown 宽松/标准/紧凑 with stored values unchanged. |
| C2 batch C part 2 | "proxies and profiles menus, profiles grid, wrapped conditions" | Proxies: node long press opens the anchored menu (选中 for a selector, 首选 for URLTest/Fallback/Smart, both checked as FlClash), the tab bar's ▾ group menu once the tabs overflow (7 rows, current checked), the delay test as the bar's primary action with a spinner while testing (no FAB), the list style in 1–3 group columns off mobile (groups taken in turn, nodes per column width, locate counts only its column). Profiles: long press opens the anchored `_menuItems` (更多 unfolds in place), add in the bar when not empty (menu anchored at the button, also from the empty state), grid of `getProfilesColumns` (≥300 wide, ≤3) with view-mode spacing, the sort sheet drags rows after a 400ms hold and saves with ✓ (no touch area on the page outside the sheet). Requests: 查看 HTTP on MITM rows opens the detail on its HTTP tab, condition chips wrap into rows Rust breaks by the block's width (no horizontal scroll). |
| C3 batch C part 3 | "user agent menu and sort, real app icons" | User agents: long press opens the anchored menu (编辑 / 删除, delete asks 「确定删除当前用户代理吗？」 and clears the global UA when it was that one), the bar's sort turns the page into a drag list with ✓ to leave (no drag area outside sort mode), add no longer selects the new UA. Real app icons: `packages.icons` in UiBridge writes `getApplicationIcon` as 96px PNGs cached by package and update time, Rust decodes them (`app_icons`) into the `AppIcons` global, used by request rows, the connection detail and access control, with the monogram until an icon arrives. |
| B3 dashboard services | "service status sheet and manage view" | Service card as `ServiceStatusCard`: service badges (Simple Icons marks from `assets/images/services`, status dot in a notch) with the status, a tap opens the sheet on a phone, ⚡ (check the shown ones, spinner) and › on wider dashboards, verdict hidden when stacked. `ServiceStatusSheet`: every enabled service with region · IP (or the node), status, tap checks one, ⚡ checks all; ⇄ opens `ServiceManageView`: switch services (the last one locked on, 显示在仪表盘 on the shown ones) and long-press drag to order, stored as `service_order` / `disabled_services` like `serviceOrder` / `disabledServices`. Checks are tracked per service. Region shows as its code, not a flag emoji. |
| Fix editor and small window | "dashboard editor drag, addable rows, small window insets" | Editor drag is owned by the column (`drag.rs`): the lifted card follows the finger (scrolling near the top or above the dock), the others slide aside once its leading edge passes their middle, measured by each card's own height, and the order changes only on release. The phone column no longer stretches by `Dock.pad`, which had pushed the 可添加 rows under the dock. Insets follow fk-box's `pushInsets`: the decor view's bars plus the part `getWindowVisibleDisplayFrame` hides at the bottom (Flyme's small window), resent on resume, focus and multi-window changes; the surface-inset guesses are gone. Default log level is error (owner's choice, as FlClash). |
| B4 dashboard sheets | "service IP sheet and runtime config preview" | A service row's address (region · IP, masked by 隐藏 IP) opens the IP quality sheet for that address (`showIpQualitySheet`): region at once, then the lookup; the status card still opens its own. The profile sheet's 👁 opens 运行时配置 (`PreviewProfileView`) over the dashboard, and back returns to the sheet. |

## Remaining

- Batch B leftovers: LiveStatusButton and its deck (needs config warnings, update download and MITM consent first),
  the 已暂停 chip (FlClash's suspend is "current SSID in the excluded SSIDs", so it lands with the on-demand page and
  its Wi-Fi SSID bridge method), the connections table on tall
  desktop windows, the 扩展脚本 value (with profile extensions).
- Phase 1 batch C, still open: developer (live status card test, update download test, both need the live status deck
  and update download first); the 「语言」 row waits on the owner. Switching to the split
  requests view already keeps the selection in the pane (`_settleDetail`, kept from C1). 添加到本地代理 in the node
  menu follows the custom proxies page.
- Phase 2: tools settings (theme extras, on-demand), advanced config (rule providers, icon sets, custom proxies, MITM),
  TUN / sniffer / NTP override pages, profile editor and extensions, QR import / TV send / preview, resources leftovers.
- Final page-by-page comparison with FlClash `lib/`.

## Waiting on the owner

- When to delete Flutter, VPN back/close behaviour, release signing, whether the build workflow skips `slint-ui`,
  kernel integration, the 「语言」 row, adding a JS engine (rquickjs) for scripts, and whether the tablet detail
  sheet buttons match the phone's.
