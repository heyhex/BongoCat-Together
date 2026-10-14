# Together settings layout verification — 2026-10-14

## Correction

The complete Together form was embedded in the field of a horizontal SettingItem. That wrapper reserves a title
column and a separate control column, so a whole-page child could exceed the actual content pane and get clipped.
The page now uses the same SettingPage/SettingGroup/SettingItem structure as the other settings pages.

Connection, keyboard/mouse sharing, dedicated text and friend display are independent outline groups. Switches and
model selection use ordinary setting rows. Invitations, application allowlists, current chords and text use vertical
full-width rows. Action rows wrap. GPUI Kit owns page scrolling and automatic narrow-pane stacking. An observation
subscription redraws the parent settings page when transient Together state changes, preserving live control state.

## Automated layout evidence

`cargo test --locked -p bongocat-ui window::together::tests`: two tests passed.

- English and Simplified Chinese, 800×600 and 1000×600: each of the four actual settings groups is rendered through
  the Settings and SettingGroup wrappers. Invitation/key/text/scale controls have nonzero size and remain between
  the measured sidebar and the window's right content margin. Long key chords use multiple lines.
- The complete four-group page receives real GPUI wheel dispatch and scrolls to its final layout actions; the final
  action row is visible inside the viewport and stays within the right margin.

The boundary tests measure rendered layout nodes. They do not substitute for Windows per-monitor DPI visual acceptance.
They use Kit's existing test-support observations, which add no layout node and are no-ops in ordinary product builds.

## Checks

Use `RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-msvc` and `CARGO_BUILD_JOBS=2` on this host.

- `just check`: passed; 1,198 passed / zero failed / five default-ignored test entries, strict Clippy and release check.
  Log: `target/together-ui-fix-check.log`.
- `just together-smoke`: passed; real QUIC and native dual-cat/settings/bubble lifecycle.
  Log: `target/together-ui-fix-native-smoke.log`.
- `python tools/validate-locales.py`: eight catalogs, 335 keys each.
- Both changelogs include the UI correction. No dependency or persisted schema changed.

The next physical-layout acceptance remains 125/150/200% Windows DPI and multiple displays.


Final artifact verification: `just dev-smoke` passed with exit 0 after rebuilding the optimized Development executable
(`target/together-ui-fix-optimized-smoke.log`). The same optimized exe completed `--together-smoke` through a waiting
process launcher with exit 0. SHA-256: `baaa2730bafa94cd0a482c784f9ddbc6acba2cb253522f3fd1b1dbbd2181a3a9`. Output: `target/release/bongocat-app.exe`.
