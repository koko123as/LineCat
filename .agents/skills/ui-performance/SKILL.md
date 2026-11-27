---
name: ui-performance
description: Use when a SeasonCat page, sheet, list, menu, or transition drops frames or feels slow, or when asked to lower UI-thread or idle load.
---

# UI Performance

## Read First

- `.agents/rules.md`: UI Frame Budget is binding; it names the usual causes.

## Workflow

1. Reproduce on a phone, with a release build and the largest data the app expects (thousands of connections, a long
   log, a profile with many groups and proxies). Desktop runs of the snapshot example say nothing about frame times.
2. Measure before changing anything: missed vsyncs from a system trace (`atrace` / Perfetto) while the motion runs.
3. Look for work nobody sees first: a model replaced instead of one row updated, a whole list built at once, a poll
   or timer left running for a hidden page, a document read on the UI thread, or data that lands in the middle of a
   transition.
4. Change one thing, install the base and the change in turn, and measure both the same way.
5. Keep the change only if it moves missed vsyncs beyond noise; otherwise revert it, however sound its reasoning.
6. Report the device, the build, the data size and the before and after numbers.
