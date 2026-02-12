# Keybinds

## Active

| Key | Action |
|-----|--------|
| Alt-C | Toggle calendar collapse — slides from left to single column (today) |
| Alt-M | Toggle monthly view — full-month grid, read-only |

## Reserved by browser — avoid these

Applies to Chrome and Edge on Windows:

| Key | Taken by |
|-----|----------|
| Alt+Left / Right | Back / Forward |
| Alt+Home | Homepage |
| Alt+D | Focus address bar |
| Alt+F | Browser menu |
| Alt+F4 | Close window (OS) |
| Alt+Tab | Window switcher (OS) |

We call `e.preventDefault()` in all handlers which blocks most browser
interference, but it's still cleaner to avoid conflicts entirely.

## AltGr note (EMEA users)

On European keyboards (FR, DE, NL, etc.) the AltGr key sends `e.altKey = true`
in the browser, which means some Alt+key combos may conflict with typing
special characters (e.g. AltGr+C on some layouts).

The flip side: EMEA users have an extra key (AltGr) that EN/US users don't.
This is an opportunity — bindings that use AltGr intentionally would give EMEA
users exclusive shortcuts without conflicting with anything. Worth revisiting
when the keybind set grows.

## Future candidates (unassigned)

| Key | Candidate action |
|-----|-----------------|
| Alt-S | Toggle services panel (slides from left rail) |
| Alt-T | Jump to today |
| Alt-W | Weekly view (default) |
