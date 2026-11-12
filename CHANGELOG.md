# Changelog

## Unreleased

- A file dropped on a long shelf no longer lands out of sight: the list scrolls down to its new row.
- A file name too long for its row is shortened in the middle, not at the end, so the extension stays in sight: "Lease agreement (s…ned).pdf" instead of "Lease agreement (sig…". The header line is cut as before.
- The full path that shows when the mouse rests on a row no longer runs off the screen: a long path wraps onto more lines within the shelf's width, whichever edge the shelf is on.
- Everything on the shelf comes off in one drag: drag the "24 items" line in the header. It moves like a single row (Ctrl copies, the right button brings up the Windows menu); files that are gone stay behind.

## 0.3.0

- Shelve checks for updates: about ten seconds after start and then once a day it asks GitHub for the latest release. When a newer version is out, a card at the top of the settings window names it and has a "Download" button that opens the release page. Nothing is downloaded or installed by itself.
- "Check for updates" under Behavior in the settings turns the check off. It is on by default, also for those who come from an older version.
- New icon: a stack of three cards on a dark blue tile, drawn pixel by pixel for the 16 and 24 px sizes so the gaps between the cards stay sharp.
- The settings window links to the project page on GitHub, at the right of the version line.

## 0.2.0

- A row dragged off the shelf with the left mouse button moves the file. Ctrl makes a copy, and the right button brings up the Windows Copy / Move menu. The row leaves the shelf once Explorer has moved the file. The header shows the Ctrl hint.
- A hidden shelf with files on it leaves a small tab at the screen edge, in the shelf's colours with an arrow; it lights up under the mouse, and resting the mouse on it or clicking it brings the shelf back.
- A left click on the tray icon shows or hides the shelf; the menu is on the right click.
- The shelf hides by one rule however it was opened: buttons up, cursor off the shelf, 1.5 s passed. Opened with the hotkey or from the tray, it first waits for the cursor to visit it.
- A sleeping network drive no longer freezes the shelf or the settings window: whether the files are still there is asked off the drawing thread.
- With several monitors, an edge that has another monitor next to it does not open the shelf.
- The shelf hides by where the mouse really is, also when a program moves the cursor or Windows drops the mouse hook.
- Hotkey errors are shown in the interface language. The counter says items, not files, since folders count too.

## 0.1.0

First public release.

- A shelf at the left or right screen edge that slides out when a file or folder is dragged there.
- Files are dragged off the shelf into folders and programs as copies; network paths work too.
- Global hotkey (`Ctrl+Alt+D` by default) and a tray menu.
- Settings window: position, hotkey, auto-show, start with Windows, language.
- English and Russian interface.
- Follows the Windows light or dark theme, accent colour and the animation setting.
- Hands unused memory back to Windows while the shelf is hidden.
