# TUI Show Layout

Task #120 requests task detail titles and layout matching `qqq show`.

Selected-task detail pane uses the existing show formatter, including its
`#ID · Status` header, section titles, blank lines, aligned field values,
description/message indentation and ascending message order. Sections are
Description, Details, Assignment, Messages, Images, History and Herdr. Empty
collections use `None`; unlinked Herdr uses `Not linked`. Created and Updated
return to separate aligned rows, as requested by the new layout reference.

TUI loads the complete local `Db::show(id)` snapshot. This reads stored Herdr
link metadata without calling Herdr. Selected-task deletion still produces the
existing unavailable notice. Refresh preserves dirty editor content and detail
scroll; selecting a different task resets detail scroll. Pane geometry stays
35/20/45 percent with current borders and padding.

Expose the existing human detail formatter within the crate. Its trusted output
escapes user control characters before adding known SGR codes. A small TUI
adapter converts only those generated codes into typed row spans, then applies
existing safe Unicode wrapping. This keeps one source of truth for titles and
layout without introducing a second show formatter or an ANSI parser dependency.
Unknown generated SGR codes use neutral style. Plain modes suppress span styles.

Checks cover header/section/field parity, all collections and stored links,
Unicode/control escaping, span roles, scrolling, refresh/deletion, cursor and
plain modes. Existing show JSON and terminal output stay unchanged.
