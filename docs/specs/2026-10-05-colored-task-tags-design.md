# Colored Task Tags

Task #168 requests color for tags in dashboard task list.

Real `[tag]` labels use existing warm yellow palette, indexed color 222. Task
description and status keep current styling. Tag spans inherit selected row
background. NO_COLOR and TERM=dumb retain plain text.

Track tags from actual task metadata, including archived prefix. Never infer
tags from arbitrary brackets in descriptions. Add one optional byte range to
each list row, plus truncation boundary for preview ellipsis. Populate ranges
after existing description/dirty metadata;
follow tag prefix through wrapped preview rows using exact Unicode character
matches. Stop at description or preview ellipsis. This preserves existing
wrapping, row count, selection/hit targets and dirty marker positions.

Dashboard renderer splits row into plain text, tagged span and plain suffix.
Dirty marker remains independently styled before description. One tag color
applies to all labels; no per-label color assignment or stored color config.

Verify rendered cells for selected/unselected rows, statuses, dirty markers,
Unicode, wrapped tags, archived tasks and literal bracket text. Existing Tags
PTY flows verify real DB metadata reaches list, live saves, NO_COLOR and resize.
Run full suite, fmt, Clippy, release build, independent review and installed
binary checks before task completion.
