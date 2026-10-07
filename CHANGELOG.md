# Changelog

## 0.3.0

- Searchable action menu (`Ctrl+G` / `F3`) with shortcuts, current settings,
  unavailable-action explanations, and mouse support.
- Scrollable contextual help (`F1`), stable candidate pane widths, match-kind
  markers, and a clickable shortcut footer with lookup and navigation status.
- Focused reading layout (`F4`) with a centered, bounded text width. Input focus
  restores the candidate pane.
- Literal, case-insensitive definition search (`/`), highlighted matches,
  cyclic next/previous navigation (`n` / `N`), and cancel-to-restore behavior.
- Definition outline (`o`), previous/next word-group navigation (`[` / `]`),
  and line scrolling (`Ctrl+J` / `Ctrl+K` while reading).
- Searchable session navigation (`Ctrl+R`) restores existing back/forward
  locations. Reading positions stay attached to source content through resizing,
  layout changes, expansion, and navigation.
- Reading layout, example/reference expansion, and pronunciation expansion now
  save automatically alongside appearance settings. `F2` controls all six
  preferences; failed writes remain visible and can be retried by another change.
- Grapheme-aware input movement, deletion, horizontal scrolling, and mouse caret
  placement, including combining marks and joined emoji.
- Mouse capture can be toggled from the action menu for native text selection.
- Cached reading layout and viewport rendering improve scrolling and reflow;
  the benchmark includes long-definition reading interactions.

The existing schema 2 dictionary pack remains compatible. Queries and navigation
history remain session-only. No new runtime dependencies or CLI flags are required.
