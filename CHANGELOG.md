# Changelog

All notable changes to Lite UPS Panel. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [1.1.0] - 2026-11-21

First public release. It is also the first version called **Lite UPS Panel** (it was "UPS Panel" in the internal 1.0.x builds).

### Added
- **Support for any USB UPS**, not only CyberPower. The HID report descriptor is scanned for the standard Power Device and Battery System usages (19 values), instead of matching a vendor ID and fixed report IDs. Units are scaled per device.
- **Brand and model** on the Status page, from the USB strings, with a vendor-name table (9+ brands) as a fallback.
- Features a UPS does not expose (transfer voltage limits, alarm, self-test) are now **disabled and greyed out** instead of failing.
- `--status` and `--dump` switches to inspect a UPS and its report descriptor, for bug reports.
- Windows 10/11 compatibility declaration in the application manifest.

### Changed
- **New dark interface** for Windows 11: Mica/Acrylic backdrop, card-based layout, rounded sidebar with distinct selected and hover states, Segoe UI and Segoe Fluent Icons.
- The UPS is polled on a **background thread**, so the window no longer freezes while a HID read is slow (a status query could take almost 3 seconds).
- The first page is built after the window is shown, so it paints correctly over the backdrop.
- List views: header drawing moved to a window subclass, column lines removed, date pickers tuned for the dark theme.
- Renamed to **Lite UPS Panel**: window title, tray tooltip, notifications, Start menu shortcut and installer. Settings and events are kept (same data folder), and installing over a 1.0.x build upgrades it.

### Fixed
- A stray NUL byte in the window code.
- The layered window lost its acrylic look without the right `supportedOS` entry in the manifest.

## [1.0.x] - 2026-11-20 and 2026-11-21 (internal builds)

Never released publicly. They covered the original scope:

- Status, Events, Energy, Configuration and UPS pages, with the screens and options taken from CyberPower PowerPanel Personal.
- Event log, power-problem summary, energy report with cost and CO₂ per country, SQLite storage.
- Shutdown policies (keep running / preserve battery) with a cancellable 30-second countdown, and a debounced state machine covered by unit tests.
- Voltage limits, alarm on/off and self-test over HID.
- Tray icon with balloon notifications, single instance, and start with Windows.
- Per-machine MSI installer built with WiX.

[1.1.0]: https://github.com/bacoinz/Lite-UPS-Panel/releases/tag/v1.1.0
