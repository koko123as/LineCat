---
name: ui-work
description: Use when changing the Slint UI in ui/: pages, sheets, menus, rows, colours and theme, icons, navigation, async feedback, or user-facing interactions.
---

# UI Work

## When To Use

Use this for user-facing changes in `ui/`: `ui/ui/app.slint`, the page logic in `ui/src/<name>.rs` and its wiring in
`ui/src/<name>_app.rs`.

## Read First

- `.agents/architecture.md`: The UI Crate and The Bridge.
- `.agents/rules.md`: UI Boundaries and UI Frame Budget.

## Workflow

1. Find the page's pair of modules: the logic module holds what the page shows and how input is parsed, the `_app`
   module the callbacks and what they publish to the window. Keep window types out of the logic module so its unit
   tests run without a window.
2. The pages follow FlClash's, page for page: match its behaviour and wording unless the task says otherwise.
3. Anything Android (files, the camera, permissions, the VPN) is a `UiBridge` command, never a JNI call from Rust.
4. Colours come from the generated theme (`ui/src/theme.rs`); change `ui/src/hct.rs` or `ui/src/colors.rs` and
   regenerate rather than editing it.
5. Check the layout with `cargo run --example snapshot -- <out dir>` in `ui/` and look at the PNGs of the pages you
   changed, at each window size the example renders.
6. Run `cargo fmt --check` and `cargo test --locked` in `ui/`. A test reads text and properties, never rendered CJK
   glyphs.
