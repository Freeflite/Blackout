# Blackout

A small Windows tray app that blacks out some or all of your monitors while the PC keeps working.

The monitors stay powered and connected. Nothing goes to sleep and no DisplayPort monitor drops off,
so the GPU and whatever is running on it (overnight LLM inference, renders, long builds) carry on
untouched. No dummy display plug needed. Where a monitor supports DDC/CI, its backlight is also
turned all the way down, and put back when the blackout ends.

## Two kinds of blackout

Which one you get depends on which monitors you've picked in settings.

| | Full: every monitor | Partial: some monitors stay on |
|---|---|---|
| For | Overnight runs | A monitor facing away, a distracting second screen |
| Wake | Tap the wake key (**Enter** by default) **3 times within 2 seconds**, the hotkey, or the tray icon | The hotkey or the tray icon |
| Keyboard | The wake key is grabbed system-wide while blacked out, so taps never reach other apps | Left alone, so you can keep typing |
| Mouse pointer | Hidden | Normal; the black windows never take focus |
| Keeps the PC awake | On by default | Off by default (normal sleep settings apply) |

The hotkey (**Ctrl+Alt+B** by default) turns a blackout on and off either way.

## Install

Needs a Rust toolchain with the MSVC build tools.

```powershell
.\install.ps1                  # build, install for this user, start at sign-in, start it now
.\install.ps1 -DesktopShortcut # also add a desktop shortcut
.\install.ps1 -Uninstall       # remove it; settings are kept
```

It installs to `%LOCALAPPDATA%\Programs\Blackout` and needs no admin rights. Run it again to upgrade.

## Use

- **Ctrl+Alt+B** blacks out and wakes.
- The tray icon: **double-click** opens settings; **right-click** for *Black out now*, *Settings…* and *Exit*.
- `blackout.exe` with no arguments blacks out now, or toggles the copy already in the tray.
- `blackout.exe --settings` opens settings, `--resident` starts in the tray, `--exit` wakes and quits.

## Settings

Changes save as you make them and apply straight away.

- **Monitors:** all of them (including ones you connect later), or just the ones you tick. **Identify**
  shows each monitor's number on its screen. You can give monitors nicknames.
- **Hotkey:** click it and press a new combination. It needs Ctrl, Alt or Win.
- **Full blackout:** the wake key (click it and press any single key), how many taps within how long
  wake it, and whether to keep the PC awake.
- **Partial blackout:** whether to keep the PC awake.
- **Both:** whether to turn the backlight down; whether to start at sign-in.
- **Appearance:** match Windows, light or dark. The title bar takes on the same colors.

## How monitors are recognized

Each monitor is remembered by the identity it reports itself (its EDID): manufacturer, model code and
serial number. That survives reboots, driver reinstalls and swapping cables between ports, and tells two
identical monitors apart. A monitor that reports no usable serial is remembered by the port it's plugged
into instead. Monitors you've set up stay in settings while unplugged, under *Not connected right now*.

## Files

| Where | What |
|---|---|
| `%APPDATA%\Blackout\settings.toml` | Settings |
| `%LOCALAPPDATA%\Blackout\brightness.toml` | Original brightness levels while a blackout is on. If Blackout is killed or the PC restarts mid-blackout, they're put back the next time it starts. |
| `%LOCALAPPDATA%\Blackout\blackout.log` | Log |

## Build and test

```powershell
cargo build --release
cargo test
cargo test -- --ignored --nocapture   # also lists this PC's monitors and whether they dim
```

The app icon is built from `assets/freeflite.png` at compile time, at every size Windows uses.

## License

BSD-2-Clause. See [LICENSE](LICENSE).
