# CircleYe

**A circular screen magnifier for Windows that opens right where you're looking.**

Tap **Shift twice** to open the lens, move the mouse to read, and tap **Shift twice** again to close it. Only the area under the circle is magnified — the rest of your screen stays exactly as it was.

CircleYe is a small, free, open-source accessibility tool. It runs quietly in the background, so there is no window to find or open every time you want to read something.

---

## Who it can help

CircleYe is designed for anyone who finds on-screen text hard to read, including:

- **People with low vision** — glaucoma, macular degeneration, diabetic retinopathy, cataracts, and other conditions that make small text difficult.
- **Older adults** who need larger text but do not want to zoom the entire screen.
- **Anyone reading small print on screen** — receipts, labels, forms, captions, spreadsheets, code, medical or financial documents.
- **Caregivers, teachers, students, and office workers** who need a quick magnifying glass without changing display settings.

It is an aid, not a replacement for medical advice or a full-screen magnifier. Many people use it alongside Windows’ built-in Magnifier.

## What it does

- Shows a **circular magnifying lens that follows the mouse**.
- **Tap `Shift` twice** to open it; tap `Shift` twice again to close it.
- **`Up` / `Down`** while the lens is open to zoom in and out.
- **`Esc`** to close.
- The rest of the screen is untouched — no panning, no lost windows.
- High-contrast ring so the lens is easy to see.
- Adjustable lens size and zoom.
- Starts with Windows (optional) and lives in the system tray.
- **No account, no ads, no tracking, no network connection.**

## Quick start

1. **Start CircleYe.** It starts at login if “Start with Windows” is enabled. You can also double-click the desktop shortcut.
2. **Tap `Shift` twice quickly.** The lens opens at the mouse pointer.
3. **Move the mouse** to read what is under the lens.
4. **Press `Up` / `Down`** to zoom in or out.
5. **Tap `Shift` twice again** to close, or press `Esc`.

> “Tap `Shift` twice” means pressing and releasing `Shift` twice in quick succession, without pressing another key in between. Typing capital letters will not trigger it.

The app runs in the background. The **ring-shaped tray icon** near the clock is where the settings live.

## The tray menu

Left- or right-click the tray icon:

- **Open lens / Close lens**
- **Zoom in / Zoom out**
- **Lens size** — 300, 400, 500, or 600 pixels
- **Start with Windows**
- **Quit CircleYe**

Double-clicking the desktop or Start Menu shortcut also toggles the lens.

## Installation

CircleYe is currently **built from source** — there is no installer package yet.

- **Windows 10 or Windows 11, 64-bit.** (The Windows Magnification API requires a 64-bit process.)
- **No administrator rights required.** It installs per user.
- CircleYe is not code-signed, so Windows SmartScreen may warn if you download a prebuilt copy. Only run builds you trust.

Build steps are below.

## Settings

Settings are stored locally at:

```text
%APPDATA%\CircleYe\settings.json
```

You can edit it while the app is closed. Delete the file to restore defaults.

| Setting | Default | Meaning |
|---|---|---|
| `lens_diameter` | `400` | Lens size in pixels |
| `zoom` | `2.5` | Magnification factor |
| `ring_width` | `5` | Ring thickness in pixels |
| `ring_color` | amber | Ring color (`0x00RRGGBB`) |
| `follow_smoothing` | `0.03` | How quickly the lens follows the mouse |
| `zoom_step` | `0.25` | Zoom change per `Up`/`Down` press |
| `min_zoom` / `max_zoom` | `1.25` / `6.0` | Zoom limits |

## Privacy

- **No telemetry, no analytics, no account, no ads, and no network access.**
- The keyboard hook only looks at key up/down events for `Shift`, `Up`, `Down`, and `Esc` so the shortcut and zoom work. It does not log keys or store what you type.
- Screen content is not saved or sent anywhere. The lens uses Windows’ own Magnification API.
- Settings stay on your computer.

## How it works

CircleYe is a small Rust application built on the Windows Magnification API. It creates a borderless, click-through circular window that follows the mouse and magnifies the screen beneath it. The high-contrast ring makes the lens easy to find, and a low-level keyboard hook provides the `Shift`-twice shortcut.

Because it uses the operating system’s magnifier engine, it works with most desktop applications.

## Limitations and known issues

- **Text can look soft at high zoom.** The Windows Magnification API applies its own smoothing, and it does not expose a filter setting to third-party apps. A sharper custom renderer is under consideration.
- **Some protected content may not magnify** — for example, UAC prompts or the lock screen — unless the app is elevated and signed. Normal desktop apps work.
- **Sticky Keys:** tapping `Shift` many times quickly can trigger Windows Sticky Keys. If this interferes, turn off the `5× Shift` shortcut in Windows Settings → Accessibility → Keyboard.
- While the lens is open, `Up`, `Down`, and `Esc` are captured by CircleYe by design.
- Multi-monitor use is basic: the lens follows the cursor, but mixed-DPI setups may behave differently.
- There is no installer yet; build from source.

## Troubleshooting

**`Shift` twice does nothing**
- Make sure CircleYe is running — look for the ring icon in the tray.
- If it is not running, start it from the desktop shortcut.
- Check Task Manager for `circleye.exe`.

**The tray icon is missing**
- Start CircleYe again from the desktop or Start Menu shortcut.

**Text looks blurry**
- Try a lower zoom (`2×` or `3×`) or a larger lens.
- This is a limitation of the Magnification API’s smoothing, not of your display.

**The app will not start**
- Confirm you are on 64-bit Windows 10 or 11.
- Check the log at `%APPDATA%\CircleYe\circleye.log`.

**How do I quit it?**
- Right-click the tray icon → **Quit CircleYe**, or end `circleye.exe` in Task Manager.

## Building from source

Requires [Rust](https://rustup.rs/) with the MSVC toolchain and the Visual Studio C++ build tools.

```powershell
git clone https://github.com/dr-loki/CircleYe.git
cd CircleYe
cargo build --release
```

Run:

```text
target\release\circleye.exe
```

To start it with Windows, copy the executable to a permanent folder (for example `%LOCALAPPDATA%\Programs\CircleYe`), create a shortcut if you like, and enable **Start with Windows** from the tray menu.

## License

MIT — see `Cargo.toml`. Contributions and accessibility feedback are welcome.

## Acknowledgements

- Built on Microsoft’s Windows Magnification API.
- Inspired by the idea of a circular reading lens, in the spirit of other low-vision tools.

CircleYe is not affiliated with Microsoft.
