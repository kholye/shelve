# Changelog

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
