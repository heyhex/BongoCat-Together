# BongoCat Together progress

Baseline: upstream v2.3.1, `30d0c34f557a76e7f265befa94e71704c813f8a9`, fetched on 2026-10-14.
Scope: Windows x64 development, two peers, original offline behavior retained. Initial implementation was local; the source-only handoff uses the separate private `heyhex/BongoCat-Together` repository, without tags, a Release or installer uploads.
Implementation decision and dependency review: [ADR-0090](adr/0090-together-p2p-boundary.md).
User instructions: [English](bongocat-together-usage.md) / [简体中文](bongocat-together-usage.zh-CN.md).

A checked item has the evidence described by that item. No phase is marked fully accepted on compilation alone.

## Phase 0 — inspection and design

- [x] Read the only AGENTS.md, manifests, input/runtime/render/UI architecture and relevant release/FFI ADRs.
- [x] Fetch upstream and establish the clean v2.3.1 baseline before changes.
- [x] Verify Iroh 1.3.0 published docs/source/examples and exact stable dependencies, licenses and Rust compatibility.
- [x] Reuse independent native overlay/runtime instances, existing model validation and prepare/commit, GPUI Kit 0.7.3.
- [x] Record the on-demand transport, authenticated invitation, strict typed protocol, privacy and configuration boundaries before implementation.

## Phase 1 — encrypted two-peer connection

- [x] Dedicated current-thread reactor, on-demand Endpoint, bounded commands, create/join/copy, nickname and disconnect UI.
- [x] Versioned expiring 256-bit invitation, authenticated endpoint pinning, rejection of wrong capability and third device.
- [x] Actual selected-path IP/relay and RTT; unknown when unavailable. Direct-only disables relays on both ends.
- [x] Real loopback QUIC integration for bidirectional probes, authorization, duplicate create, disconnect and reconnection.
- [x] Reliable frame reader survives metrics/control selection without cancellation of partially read frames.
- [ ] Two physical Windows machines on different public networks; successful public relay route and network-switch acceptance.

## Phase 2 — independent friend cat

- [x] Independent Windows runtime/native window/GPU; no duplicate Raw Input/gamepad/shortcut service.
- [x] Background model preparation; failed candidates preserve current cat; loading/error notification/retry are typed real states; imported models reuse the environment-scoped store.
- [x] PeerInfo sends model identity/source and scale; available presets match automatically, missing/custom assets fall back locally.
- [x] Local model/scale overrides, visibility, position swaps/reset, process-local position memory, right/left initial placement.
- [x] Native smoke draws two Live2D cats, changes model, verifies scale/visibility/swaps, and removes friend after disconnect.
- [ ] Physical drag, per-monitor DPI and side-by-side visual acceptance on 125/150/200% displays.

## Phase 3 — authorized keyboard and mouse

- [x] Reliable typed down/up, physical HID chords, local current-key labels, abstract activity without key codes.
- [x] Default-off independent controls, foreground executable allowlist, priority privacy revision, pause/stop/reset.
- [x] Persistent sharing indicator and stop button; closing or losing the badge revokes sharing.
- [x] Reliable button edges, normalized latest-only ~30 Hz mouse datagrams, independent sequence/revision and existing smoothing.
- [x] Wheel direction is validated and briefly displayed; click semantics remain model-dependent. No artificial preset wheel/double-click motion.
- [x] Runtime-isolation and actual QUIC-to-runtime contract tests, precise chords/releases, duplicate/gap/privacy validation,
  5,000 latest cursor updates, abstract mode, pause, queue-overflow revoke and prioritized shutdown.
- [ ] Physical input, foreground application switches, Ctrl+Alt+A/Win+L/PrintScreen, lock/sleep/UAC/permission-loss acceptance.

## Phase 4 — dedicated text sharing

- [x] GPUI Kit Textarea, default off, dedicated field only, Unicode/multiline, 120 ms debounce, 512-scalar limit.
- [x] Ordered replacement/edits/deletion, revoke/clear/reconnect reset; no other application text or history.
- [x] Theme-following friend bubble, ten-second expiry and bounded scrollable text area.
- [x] Real QUIC Unicode/emoji edits and native GPUI bubble content/viewport smoke.
- [ ] Real Chinese IME composition/candidate-selection and manual editing acceptance; Unicode transport is separate evidence.
- [ ] A separate bubble-style editor, if required beyond application light/dark/system themes.

## Phase 5 — stability and delivery

- [x] `just check`: formatting, all strict Clippy feature groups, workspace tests and release check passed (bounded build concurrency).
- [x] Windows `just together-smoke`: real direct QUIC, two rendered cats, missing-model fallback, peer model switch,
  layout, failed-model rollback/retry, isolated remote keys, Unicode bubble, indicator-failure revoke and disconnect cleanup.
- [x] Original optimized `just dev-smoke`: offline startup/settings close-reopen/runtime continuity and normal quit passed. Final optimized executable refresh is recorded below.
- [x] Paired README/CHANGELOG and usage guides; Technical Design/TODO/ADR synchronized.
- [ ] Cross-public-network/relay/network-switch acceptance, physical input/IME/DPI, resource baselines and eight-hour soak.
- [ ] Signed install/upgrade/uninstall acceptance and Cubism redistribution/release gates. No production installer is delivered.

## Verification log — 2026-10-14

All Rust invocations use `RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-msvc` explicitly.
The host's default GNU toolchain is not changed. Existing production data is not used.

- `cargo test --locked -p bongocat-network`: 11 tests passed, including real encrypted loopback peers.
- `cargo test --locked -p bongocat-app --lib together_`: 2 tests passed (independent runtime keys; foreground/reset and remote releases).
- `cargo test --locked -p bongocat-i18n --lib`: 14 passed after replacing dynamically composed keys with explicit literals.
- `python tools/validate-locales.py`: 8 locales / 329 keys each validated.
- `just together-smoke`: passed on Windows, including PeerInfo missing-model fallback/switch and unavailable-indicator revocation.
  Evidence: `target/together-native-smoke.log`, including explicit failed-model rollback/retry. This checks native windows and view state, not screenshots or manual IME.
- Initial full check caught a missing smoke-harness option in the existing binary contract; fixed in `binary_tests/product_options.rs`.
- One full workspace run hit the unchanged runtime FPS timing threshold under load (45.5 FPS vs 54 required).
  Isolated exact test passed: `target/together-pacing-retest.log`; a subsequent workspace test run also passed.
- A later full check passed formatting, all Clippy groups and workspace tests, then release checking exited inside upstream
  `aho-corasick` without a Rust diagnostic. Retrying with `CARGO_BUILD_JOBS=2`, without changing compiler/profile/source; the bounded-concurrency full check passed.
- `python -m unittest discover -s tools/tests`: 101 passed, 1 failed. The failure expects Unix `/` paths on Windows in
  `test_forbidden_unions_and_linux_are_reported`; repository cfg scanning itself passes. Reproduced that one test by executing
  pristine `git show HEAD:tools/tests/test_supported_platform_cfg.py` source, with no local changes to tools/tests.
  Logs: `target/together-python-contracts.log`, `target/together-upstream-python-repro.log`.
  PyYAML 6.0.3 was installed only under ignored target for this verification and removed afterwards; it is not an application dependency. The temporary cargo-deny download was also removed after its audit passed.
- Downloaded the official cargo-deny 0.20.2 Windows x64 release and verified its published SHA-256
  `975a22143262fd27476d19ee00c7af67978426e40e1dee94eed6bbade1cf87dc` before execution.
  `cargo-deny --locked check licenses sources`: both passed against deny.toml; `target/together-dependency-audit.log`.
  This was a temporary local verification tool, not a new application dependency or a global installation.
- Targeted changelog/packaging/version/repository-cfg Python contracts: 51 passed; `target/together-contracts-final.log`.

## Remaining limits and next acceptance

The current result is a runnable local development build, not a daily-use/release certification.
No two-machine public-network environment is available. Public relay and changing networks remain unverified.
No Chinese IME, physical input lifecycle, DPI or eight-hour measurements are claimed. No macOS Together implementation is claimed.
Friend preferences/allowlists are process-local; original persisted v1 schemas and fixtures remain unchanged.
New copy is complete in Simplified Chinese and English; several other locales temporarily use English for Together.
Model-specific wheel/double-click animations and independent bubble styling are not added to the original presets.
The next external acceptance is two Windows machines on different networks, followed by IME/input/DPI and soak/install gates.


Final-tree update: strict Clippy groups, workspace tests, release check and the extended Together native smoke
passed again after model loading/error/retry status was added. The original optimized `just dev-smoke` also
passed with exit code 0 (`target/together-offline-smoke.log`). A final optimized refresh uses the same recipe,
with `CARGO_BUILD_JOBS=2`; no release profile or packaging rules were overridden.


Final full-check count: 1,196 passed, zero failed, five default-ignored entries across 47 test result groups.
Ignored entries require real audio, an OS-wide hotkey, Development startup-item mutation, interactive input,
or the crash-probe child that the parent test spawns separately. These are not represented as new physical-input evidence.


Final optimized refresh: `just dev-smoke` passed with exit 0; `target/together-offline-smoke-final.log`.
The same optimized executable completed `--together-smoke` under a waiting process launcher, exit 0.
A bounded startup-inclusive offline resource observation and executable SHA-256 are recorded in
[Windows development verification](benchmark/together-windows-development-2026-10-14.md).
It does not fulfill dual-cat network/resource/soak acceptance.


## UI clipping correction

User feedback: the right-side Together controls were cut off. The entire form had been placed inside a single
horizontal SettingItem's field. Replaced that wrapper with native connection/input/text/friend groups and individual
setting rows, keeping the existing command handlers and model/input/network boundaries. Long fields use vertical
full-width rows, buttons wrap, and Settings owns scrolling/responsive stacking. Added a parent observation subscription
for transient Together state. Regression verification targets English/Chinese 800×600 and 1000×600 content bounds,
long key-chord wrapping, and scrolling to the last layout controls. Actual DPI/monitor visual acceptance remains separate.


UI correction verification: both GPUI layout regressions passed through the actual Settings/SettingGroup wrappers,
covering English and Simplified Chinese at 800×600 / 1000×600, wrapped long chords and scrolling to the last actions.
`just check` passed again (formatting, strict feature-group Clippy, workspace tests and release check);
evidence: `target/together-ui-fix-check.log`. Locale validation: 8 catalogs, 335 keys each. The UI uses the standard
outline groups, spacing and automatic vertical stacking supplied by GPUI Kit. No network protocol, persisted schema,
permission defaults, original offline pages or release profile was changed by this UI correction.

UI correction delivery: the updated optimized Development exe passed `just dev-smoke` and the waiting optimized
`--together-smoke` launch, both exit 0. Details and artifact hash:
[settings layout verification](benchmark/together-settings-layout-2026-10-14.md).

## Private source handoff — 2026-10-14

Destination: `heyhex/BongoCat-Together`, private. Retain upstream history and the
existing `origin`; use the separate `together` remote for this development work.
This is an initial source upload, not a product release. Do not push tags or
upload build outputs, installers, local data or logs. Actions are disabled on the
new repository so inherited release and Gitee synchronization workflows do not run.

The English and Simplified Chinese READMEs now describe Together first, include
source startup and explicit sharing/acceptance limits, preserve upstream credits,
and use a local static SVG concept illustration. Upstream downloads, popularity
badges and donation/community graphics are not presented as this repository's own.
The homepage illustration is not an application screenshot. This documentation
refresh adds no application behavior and needs no separate changelog entry; the
Together behavior and settings fix already have paired Unreleased entries.

Upload verification:

- `just check`: passed again, including formatting, strict Clippy feature groups,
  workspace tests and release checking (`target/together-upload-check.log`).
- `just together-smoke`: passed again with real local encrypted QUIC, two native
  overlays, isolated friend keys, Unicode text and disconnect cleanup
  (`target/together-upload-native-smoke.log`).
- Locale validation: 8 catalogs / 335 keys each. Release changelog contracts:
  12 passed. Both README image/SVG audits passed; local GitHub-rendered previews
  were inspected at desktop and 360px widths, including a dark background.
- The optimized `just dev-smoke` rerun did not execute: Windows returned error
  740. The existing per-user compatibility entry for that exact optimized exe
  specifies `RUNASADMIN`; the embedded manifest remains `asInvoker`. That user
  setting was not changed. The earlier optimized offline smoke evidence above
  is historical, not a successful rerun for this upload.

The real-world acceptance and release gates listed above remain outstanding.
