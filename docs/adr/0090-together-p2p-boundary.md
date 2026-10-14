# ADR-0090: Optional two-peer Together sessions

Status: accepted for incremental development, acceptance pending
Date: 2026-10-14

## Decision

Keep all existing offline input, models, configuration, update and rendering
paths. Add `bongocat-network` only when there is an implemented transport and
automated contract. Its worker owns ephemeral Iroh endpoints and a bounded
command queue. No GPUI entities, native handles or GPU objects enter it.
Public API uses project-owned command/snapshot types; Iroh/QUIC types remain
private. The GPUI page consumes this nonblocking typed service; the app owns
and shuts down the service, independently of settings-window lifetime.

Use Iroh 1.3.0, verified from official published documentation and examples
on 2026-10-14: https://docs.rs/iroh/1.3.0/iroh/ and
https://github.com/n0-computer/iroh/tree/v1.3.0/examples . It is maintained by
n0, MIT OR Apache-2.0, requires Rust 1.91, supports x64 Windows, uses QUIC/TLS
authenticated endpoint public keys. Select ring TLS and port mapping; avoid
metrics and Apple acceleration on this Windows MVP. Iroh includes reviewed
upstream unsafe/OS/crypto internals; our service forbids unsafe Rust. Replacement
cost is the private transport adapter, not runtime or render changes.

Tokio 1.53.2 (MIT) supplies Iroh's required asynchronous reactor on one dedicated
thread, never on the UI/render/input owner. Base64 0.23.1 (MIT OR Apache-2.0,
default unsafe SIMD disabled) encodes bounded invites. Getrandom 0.4.3 (MIT OR
Apache-2.0) supplies OS randomness for 256-bit authorization and session IDs.
Serde/serde_json and async-channel reuse workspace dependencies. These are
active maintained stable crates; all are precisely pinned. No new persisted
configuration fields are required for the first connection closure.

Invites carry a version, endpoint/address hints, a session ID, a random
authorization capability and a limited initial-join lifetime. They are secrets,
not logs. A successful first join pins the authenticated QUIC endpoint ID.
Subsequent joins from a different endpoint are rejected even with the invite.
Manual room termination closes the endpoint and destroys its capability.
The host is authenticated by the endpoint ID in the invitation. Receivers bound
and validate frames before allocation/deserialization and use a reliable ordered
stream for edges/control/text. A selected live IP/relay path determines the
display; no path means unknown. Direct-only disables relay transport on both
ends rather than relabeling a relay as direct. n0 public discovery/relays are
third-party infrastructure and are described accordingly.

Precise keys and text default off. No global keystroke-to-text reconstruction,
OS event injection, network model transfer, remote commands or history storage.
Wire events require explicit permission, monotonic sequence and session binding.
Revoke/disconnect clears pressed/text state; reconnect exchanges a snapshot.
Mouse positions use latest-value/coalescing, not the reliable edge backlog.

Reuse an independent native overlay and cat runtime rather
than changing the renderer to draw several cats. The friend overlay must skip
local global-input registration and shortcuts. A per-cat model owner preserves
model-switch rollback. Missing peer assets use the bundled default locally.

## Acceptance

Progress/evidence live in `docs/bongocat-together-plan.md`. Loopback QUIC tests
do not prove NAT traversal across two public networks; UI compilation does not
prove Windows rendering or Chinese IME. Existing Cubism/installer/signing and
soak gates remain applicable, separate from local development.


## Implemented display and lifecycle contract

PeerInfo contains a bounded portable model ID, built-in flag and scale hint; no model path or resource payload is accepted.
Resolution runs in the background model loader. Unknown presets and custom assets fall back to standard; custom models
are explicitly selected from the local validated store. Local model/scale overrides win over subsequent peer hints.
Friend layout/allowlist choices remain process-local, avoiding any persisted schema changes in this development slice.

The friend GPU/window is ticked outside GPUI App borrows. A separate GPUI Root popup presents nickname/current keys,
wheel direction and dedicated text, following the application theme. Text expires after ten seconds and long text scrolls
inside a bounded area. The original model has no wheel-specific animation; no unsupported motion is claimed.
Hiding the friend hides its content while preserving an active local sharing indicator. Close/open/update failure revokes
sharing through the independent privacy watch. A bounded platform positioning worker joins before its window closes.
Mouse datagrams carry both monotonic sequence and share revision so old authorization cannot replay positions.
The wire reader runs as a session-scoped task to avoid partial-read cancellation; teardown aborts and releases it.

The full workspace dependency update selected num-modular 0.6.5 through num-order 1.2.0 → handlebars 6.4.4 →
cargo-packager 0.11.8 (packaging and update development dependency), as verified with cargo tree --invert.
This does not add that dependency to the network public API.


Dependency verification on 2026-10-14: official cargo-deny 0.20.2, release SHA-256 verified;
`--locked check licenses sources` passes against the unchanged repository deny.toml.
Changing mouse/keyboard sharing while text remains enabled resends only the already authorized current text.
Missed privacy revisions, pause or text-off erase the transient cache rather than redisclosing previous text.


The Together settings surface uses the same native SettingPage/SettingGroup/SettingItem structure as other pages.
A complete page is not a horizontal setting row's field. Connection, input sharing, text and friend-display sections
use standard groups, vertical full-width rows for long fields, and wrapping action controls. SettingsView observes the
transient Together entity so snapshot/control changes update the actual native setting rows without a second UI framework.
