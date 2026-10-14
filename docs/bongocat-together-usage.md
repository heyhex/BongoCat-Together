# BongoCat Together guide

Together is currently a Windows x64 development feature for two people. No account or self-hosted server is needed.
Local cat interaction remains available offline.

## Start and connect

In the checkout, run PowerShell with the MSVC toolchain and use the existing development entry point:

```powershell
$env:RUSTUP_TOOLCHAIN='1.97.1-x86_64-pc-windows-msvc'
just dev
```

The optimized Development `target/release/bongocat-app.exe` can run directly from this workspace; `target/debug/bongocat-app.exe` is also available for debugging. It needs the checkout's model resources;
it is not a standalone portable distribution. Development builds use separate data from the production application.

Open settings from the tray and select **Together**. Enter a nickname, create a room and privately share the copied invitation.
Your friend enters their nickname and invitation, then joins. Initial invitations expire after ten minutes; ending the room
invalidates them. The first successful guest's authenticated endpoint identity is pinned, so restarting the guest application
requires a new invitation. Either peer can end the room. Unexpected disconnects trigger bounded reconnection attempts;
sharing must be explicitly enabled again after reconnection.

Normal mode may use n0 public discovery and relay infrastructure. QUIC encrypts the transport; there is no account or
application text-storage server. Direct-only mode disables relays and requires a reachable UDP path; NAT/firewalls may
prevent connection. Path and RTT show unknown when unavailable. Keep invitation codes private.

## Sharing controls

All sharing starts disabled. Your own cat continues using its existing local input settings.

- Abstract keyboard sharing sends activity without key codes.
- Precise sharing sends physical key down/up and chords. These are distinct from IME-produced text and may reveal input.
  A persistent desktop indicator provides a stop button while sharing.
- The executable allowlist accepts basenames such as `code.exe; notepad.exe`; Enter or blur applies it. Empty means unrestricted.
  It filters precise keys, not password fields inside applications.
- Mouse sharing sends normalized motion, button edges and wheel direction, never screens, files or window contents.
  Animation depends on model support. Presets have no dedicated scroll/double-click motion; the bubble briefly shows scroll direction.
- Text sharing uses only the dedicated multiline field, up to 512 Unicode scalars. Edits/deletion replace the current text.
  Bubbles expire after ten seconds, reappear on edits, and scroll within a bounded height. Disabling text clears the remote copy.

Pause clears remote state while retaining switches; resumed sharing starts with new input. Stop disables everything.
Closing or losing the indicator, input Reset, queue overflow and disconnection stop sharing. No input/text history is logged
or saved, no other application's text is read, and received events never become operating-system input.

## Cats and models

Each cat has independent state. Use the original window gestures to drag them, and Together controls to select a local
friend model, change scale, swap positions, hide the friend, or reset layout. Failed model preparation preserves the active cat and shows a notification with a retry button; selecting another local model also recovers.
Disconnecting removes the friend and leaves normal local cat interaction running.

Peer model IDs and scale hints are synchronized. Available built-in models match automatically; missing/custom assets use
the default cat. Assets are never automatically transferred. Import custom models through the original model library and
explicitly select the local friend model. Manual model/scale choices override later peer hints. Friend display preferences and
the allowlist last for the application process, and reset after restart. Hiding the friend also hides their bubble, while keeping
a local sharing indicator if needed.

## Acceptance still pending

Automated tests exercise real loopback QUIC, authorization/third-device rejection, reconnection, events and revocation.
The native Windows smoke exercises dual Live2D windows, model switching, scale/visibility/position swaps, isolated remote
keys, Unicode bubbles and teardown. This does not establish cross-network, real Chinese IME composition, physical input,
lock/sleep/UAC, DPI/multiple-monitor or eight-hour soak acceptance. macOS Together is unavailable; some new copy in other
locales remains English. Bubbles follow the application theme and have no separate style editor yet.

There is no signed production installer satisfying Cubism redistribution and installation/upgrade/uninstall gates.
See the [phase record](bongocat-together-plan.md) for evidence and remaining work.
