# Using Wordglide

## Choosing and downloading a dictionary

`--data` selects an unpacked pack directory. Otherwise, the reader checks
`WORDGLIDE_DATA`, the managed download, `english-pack/` beside the executable,
then the user-data directory under `dict/english`. An explicitly selected invalid
pack is diagnosed rather than replaced silently.

F2 Settings and `--download-data` install the official dictionary with progress,
cancellation and checksum verification. The first installation enables lookup
immediately. Updating an open dictionary preserves the session; restart to use
the newly installed pack. Failed or cancelled installations keep the active pack.
Custom packs are opened with `--data`; they are not registered with the downloader.

Managed downloads live under `downloads/packs`, selected by `downloads/current.json`.
The base directory is `$XDG_DATA_HOME/dict` (or `~/.local/share/dict`) on Linux,
and `~/Library/Application Support/org.wordglide.dict` on macOS. Retained
installations consume space until removed manually. For manual official installs,
extract `english-pack.tar.xz` so `english-pack/` sits beside the executable.

`--no-color` or nonempty `NO_COLOR` disables colors. `--no-mouse` keeps native
terminal selection. You can also toggle mouse capture through the action menu.

## Finding actions

Press **Ctrl+G** or **F3** to search available actions. Type or paste a filter,
use ↑/↓ to select, and press Enter or click a row to run it. Unavailable actions
show their reason; Esc cancels without changing your lookup. Existing shortcuts
still work. F1 help scrolls with ↑/↓, PageUp/PageDown, Home/End, and the wheel.
The bottom line's shortcuts are clickable.

The candidate column stays the same width as you type. `=` marks exact matches,
`→` word forms, and `≈` spelling suggestions. The status line shows focus,
match type, candidate position, and available back/forward locations.

The action menu also toggles mouse capture for the current session, allowing
native terminal selection without restarting. `--no-mouse` selects its initial
state. Idle sessions redraw only when input, a result, or the terminal size changes.

## Rich source content

Word forms retain their source grammar, dialect and historical labels. Source
synonyms and antonyms appear under the sense that actually owns them; word-level
relations keep their source qualifiers. Derived and related terms keep distinct
source categories. Forms and related phrases are whole keyboard/mouse lookup
targets when present in the installed dictionary.

In reading focus, **v** expands all forms, **r** expands all term relations, and
**t** expands the original etymology text. Forms and each relation category show
four items by default; omitted counts indicate more. Etymology starts collapsed.
These three toggles are session-only; actions and F1 help list them. **p** retains
its saved short/full IPA preference and now includes source dialect labels/notes.
Expand/collapse, find, resizing, follow and back/forward preserve source positions.
Missing source fields stay missing, and related words are not interchangeable
in every context. No definitions or examples are generated.

This version supports English dictionaries and offers no language selector.

## Reading long entries

In reading focus, **/** finds text in the displayed definition. Type or paste a
literal, case-insensitive query, then Enter to keep it or Esc to restore your
previous reading position. ↑/↓ in the search prompt moves through matches;
**n/N** repeats after closing it. Hidden examples and IPA are not searched.
Changing words clears the search. Follow hints temporarily take precedence over
search highlighting.

Press **o** for the word / part-of-speech outline, or **[ / ]** for the previous /
next group. For entries with multiple etymologies, section headers and outline
rows display `[Etym n]` branches, and opening the outline preselects the active
section. **Ctrl+J/K** scrolls one line down/up; ↑/↓ continues selecting
candidates. PageUp/PageDown and the wheel retain their existing behavior.
**F4** toggles split / focused reading. Focused reading hides candidates while
you read; returning to input shows them again. Lines are at most 96 columns.

Press **Space** in reading focus (or **Alt+P** from any focus) to open a floating
**candidate peek card**. The peek card previews the selected candidate's headword,
part of speech, primary definition, and an example on demand without altering your
scroll position or session history. Press ↑/↓, Ctrl+P/Ctrl+N, Tab/Shift+Tab,
or Home/End to compare other candidates. Press **Enter** or click the card to
read that entry in full; press **Esc** or click outside to dismiss the card and
return to your reading place.

Resizing, expanding examples/IPA, and switching layouts retain your current
source sense where possible. Collapsing an example you were reading returns to
its sense. **Ctrl+R** searches this session's existing back/forward locations;
Enter or a click restores the query, candidate, focus, and reading position.
It does not save your queries or add a log of everything you type.

## Settings and appearance

Press **F2** from either pane to open Settings. Use ↑/↓ to choose a
setting and ←/→ or Space to change it; clicking a setting changes it, and the
wheel selects a setting. Changes apply immediately and save automatically.
Layout, examples/references, and IPA preferences also save automatically, including
changes made with F4, e, and p. Esc or F2 closes the panel and keeps your choices. Enter closes a preference row;
on the download row, Enter or Space starts the download. Lookup, selection,
reading position, and navigation history are preserved.

Nine built-in palettes are available: `default` (terminal colors with cyan
accents), `orange` (black and warm amber), `gruvbox_light` (cream),
`gruvbox_dark_v2` (warm dark gray), `whiteout` (white with blue accents),
`nord` (arctic cold blue and frost cyan), `tokyo_night` (deep indigo night
with sky blue accents), `dracula` (dark purple with green highlights), and
`sepia` (warm parchment with roasted coffee text).
Headwords are emphasized separately from part of speech and IPA; examples and
source references use secondary colors. Panes have rounded borders.

**Theme background** uses the palette's background when on; off uses your
terminal's background, including any transparency configured in the terminal.
Selected rows and follow hints retain their local backgrounds. The `default`
palette always uses the terminal background. Dark text from a light
palette can be hard to read on a dark terminal background, and vice versa;
choose a palette that matches your terminal when theme background is off.

**Truecolor** uses RGB colors when on; off converts palette colors to xterm 256
colors. Both switches default to on. The default palette uses ANSI colors in
either mode. These per-session arguments override saved preferences:

```sh
wordglide --theme gruvbox_dark_v2
wordglide --theme orange --theme-background=false --truecolor=false
```

`--no-color` and non-empty `NO_COLOR` take priority over appearance preferences:
the screen uses terminal colors, bold, italics, and reverse video. F2 still lets
you save preferences for a future colored session.

Preferences live in `config.json` in the platform user configuration directory:
`$XDG_CONFIG_HOME/dict` (or `~/.config/dict`) on Linux, and
`~/Library/Application Support/org.wordglide.dict` on macOS. The file is created
only when you change a setting. Appearance and reading defaults are:

```json
{
  "color_theme": "default",
  "theme_background": true,
  "truecolor": true,
  "reading_layout": "split",
  "expand_examples": false,
  "expand_ipa": false
}
```

Launch arguments alone do not write this file. Changing a setting in F2 saves
only that field, preserving unrelated launch overrides as session-only choices.
If a save fails, the current session keeps the change and the panel reports
"Not saved"; a later change retries all unsaved edits. Invalid configuration
reports its path before the terminal UI opens and leaves the file untouched.
`--info` and `--verify-data` ignore appearance configuration. Custom themes and
btop `.theme` imports are not supported.

Input movement and deletion respect Unicode graphemes, including combining
marks and joined emoji. Clicking a prediction places the cursor at the end of
real input; it does not accept the prediction.

## Verify a data pack

Normal startup checks schema, file lengths, compact-index and entry-locator headers. It does not hash the dictionary,
walk the vocabulary, or decompress definition blocks.
The small, already-loaded FST buffer receives its built-in CRC check to reject
accidentally damaged nodes before traversal.

For complete verification, run:

```sh
wordglide --verify-data --data PACK_DIRECTORY
```

This checks all SHA-256 receipts, vocabulary/index agreement, prebuilt ranking,
block/locator coverage, every compressed block and entry, and entry/index headword, score, and
part-of-speech agreement, prints the entry count and elapsed time, then exits without
opening the TUI. Omit `--data` to verify the automatically selected pack.
Successful verification exits with status 0; errors exit with a nonzero status.
Use a schema-5 pack compatible with the reader. `--data` and
`WORDGLIDE_DATA` take precedence over managed downloads.

To inspect a pack without verifying it:

```sh
wordglide --info --data PACK_DIRECTORY
```

This prints the pack path, schema, entry count, source snapshot, provenance, and
data and code licenses.

## Keys

| Key | Action |
| --- | --- |
| Type / paste | Incremental lookup while input is focused |
| ↑ / ↓ or Ctrl+P / Ctrl+N | Select candidate; preview follows |
| PageUp / PageDown, or the wheel | Scroll the definition by one visible page; works while hints show |
| Tab | Complete common prefix, then cycle a fixed candidate list |
| Shift+Tab | Cycle completions backward |
| → at input end / Ctrl+F | Accept the gray suggestion |
| Enter | Accept candidate and focus definition |
| Ctrl+L | Switch input / definition focus |
| Home / End in definition | Jump to the start / end of the definitions |
| `f` in definition | Show two-letter hints on visible dictionary words |
| Hint letters | Follow that word, without Enter |
| `e` in definition | Examples and references: compact / full |
| `p` in definition | Pronunciation (IPA): short / full |
| `v` in definition | Word forms: compact / full |
| `r` in definition | Term relations: compact / full |
| `t` in definition | Etymology: collapsed / expanded |
| `?` in definition / `F1` | Show the key help; Esc closes it |
| Ctrl+G / F3 | Search and run available actions |
| / in definition | Find displayed text; Enter keeps, Esc restores position |
| n / N in definition | Next / previous match |
| o in definition | Jump through the definition outline (shows [Etym n] branches) |
| Space in definition / Alt+P | Peek candidate card; ↑/↓ compare, Enter read, Esc close |
| [ / ] in definition | Previous / next word or part-of-speech group |
| Ctrl+J / Ctrl+K in definition | Scroll one line down / up |
| F4 | Toggle split / focused reading |
| Ctrl+R | Search session back/forward locations |
| `F2` | Settings: appearance, reading preferences, and dictionary download/update |
| Click in definition | Focus the definition pane |
| Click a word in the focused definition | Follow that word |
| Click a candidate | Select it and preview it without changing focus |
| Click the highlighted candidate again | Accept it and focus the definition |
| Click in the input box | Focus input and place the cursor at the clicked grapheme |
| Esc | Undo active completion, cancel hints, or return input focus |
| Ctrl+Z | Return to the previous query, selection, focus, and scroll |
| Ctrl+Y | Go forward again after going back |
| Ctrl+U | Clear input for another lookup |
| Ctrl+W / Alt+Backspace | Delete the previous word |
| Ctrl+K | Delete from the cursor to the end |
| Ctrl+C | Exit |
| ← / →, Home / End, Ctrl+A / Ctrl+E | Edit input position |
| Ctrl+← / Ctrl+→ | Move the cursor by one word |
| Alt+← / Alt+→ | Step history back / forward |

Gray suffixes are displayed only for prefix suggestions at the end of the
input. The selected candidate supplies the prediction when it extends the query;
otherwise the first longer prefix candidate supplies it. The prediction prefers
a plain single word over a hyphenated or multi-word compound, and is hidden when
it does not score above an exact match of the query, so an exact match can stay
selected and previewed without a noisy suggestion. Right/Ctrl+F accepts the
prediction; Enter accepts the selected candidate. Typing still triggers immediate
previews without accepting a suggestion.
Tab completes the whole dictionary query, including phrases, without adding a
space. Multiple matches first extend to the common prefix of all prefix matches,
including words outside the visible top 20. Further Tab presses cycle the fixed
ranked list and fill the selected word. Esc restores the pre-completion query;
editing ends completion and starts a fresh lookup. Enter is optional for lookup
and only accepts the selected word to enter reading. Completion does not add
persistent history or personalize ranking.

Hints highlight the first two letters of a visible word and do not shift the
text. Only words present in the local pack receive hints; one-letter tokens do
not. Navigation history lives in the current session: `Ctrl+Z` steps
back through followed words and `Ctrl+Y` steps forward again; a new lookup or
follow clears the forward steps. Narrow terminals use stacked panes; the minimum
usable size is 30×10.

Mouse works in two steps: the first click inside the definition pane focuses it,
and clicking a visible word there follows it like a hint. Clicking the input box
returns focus and places the cursor at the clicked grapheme; clicks on a
prediction place it at the end of the real input. The wheel scrolls the definition
from any focus. A click on a candidate selects and previews it without moving
the focus; clicking the highlighted candidate again accepts it, like Enter.
Only words present in the local pack respond, and the jump
reuses the same session history as `Ctrl+Z`. Reporting is limited to presses and
the wheel, so pointer motion is never sent; terminals that still route
drag-selection to the application need their bypass key (usually Shift) for
native copy.
