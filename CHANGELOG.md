# Changelog

## v1.1.4 — 2026-08-25

- Removed the long black frame when entering or leaving borderless fullscreen.
- Ignore transient Windows occlusion notifications unless the window is actually minimized.
- Preserve usable suboptimal WGPU frames during fullscreen surface transitions.

## v1.1.3 — 2026-08-25

- Fixed a WGPU validation crash that occurred shortly after minimizing the window.
- Pause rendering while minimized or occluded, then safely reconfigure and resume after restore.
- Drop suboptimal surface frames before reconfiguring the presentation surface.

## v1.1.2 — 2026-08-25

- Fixed the taskbar icon for portable copies launched directly from File Explorer.
- Added a stable Windows AppUserModelID and explicit relaunch icon resource.
- Reapplied the window and taskbar icons after native window creation.

## v1.1.1 — 2026-08-25

- Fixed the Windows taskbar icon by assigning the application icon to both the window and taskbar icon slots.

## v1.1.0 — 2026-08-25

- Added a unified, scrollable settings interface with dedicated CLOCK, THOUGHTS, and DATA sections.
- Added 12/24-hour time, date format, clock size, seconds, animation speed, and phrase visibility controls.
- Added a persistent custom phrase library with add, delete, and TXT import support.
- Added configurable phrase timing and sequential or random playback.
- Added guarded deletion of local user files with immediate restoration of defaults.
- Refined menu animation, spacing, clipping, high-DPI scrolling, and visual consistency.
- Improved system media controls and fullscreen behavior.

## v1.0.0 — 2026-08-18

- Initial native Rust and WGPU release.
