<p align="center">
  <img src="assets/readme/hero.svg" width="100%" alt="BongoCat Together — two desktop cats connected by an invitation. Concept illustration, not an application screenshot.">
</p>

# BongoCat Together

**Two cats on your desktop. One invitation to bring a friend along.**

English · [简体中文](README.zh-CN.md) · [Usage guide](docs/bongocat-together-usage.md)

A Windows development version of [BongoCat](https://github.com/ayangweb/BongoCat),
with optional encrypted two-person rooms. Keep your own cat and show a friend's
cat beside it; choose whether to share keyboard activity, mouse actions, or text
typed into a dedicated field.

**Development build · Windows x64 · No installer or Release published for Together.**

## One invitation, two independent cats

1. **Create a room** and send its invitation privately to one friend.
2. **Join the room** to display a second cat with its own model, size and position.
3. **Choose what to share.** Every sharing control starts off, and a visible indicator lets you stop sharing.

No account or self-hosted server is needed. Connections use encrypted QUIC through
Iroh; public discovery and relay services may help peers connect. Direct-only
mode disables relays and requires a reachable UDP path. The local cat works offline.

| What you can share | What the friend receives |
| --- | --- |
| Abstract keyboard activity | An activity signal without specific key codes |
| Precise keys | Physical key presses, releases and combinations |
| Mouse actions | Normalized movement, buttons and wheel direction |
| Dedicated-field text | Only text you type into the sharing field; no other app text or clipboard |

Remote input animates the friend cat; it does not control the other computer.
Text bubbles expire, and shared keys and text are not stored in a history or logs.
Precise key sequences can reveal what you type: enable them deliberately, optionally
limit them to selected foreground applications, and use the stop button when finished.

## Try it from source

Use **Windows 10 1903+ x64**, Rust with the **MSVC** toolchain, Visual Studio C++
Build Tools with a Windows SDK, and [just](https://github.com/casey/just).
Keep the checked-out model and vendor resources with the workspace.

```powershell
git clone https://github.com/heyhex/BongoCat-Together.git
cd BongoCat-Together
$env:RUSTUP_TOOLCHAIN='1.97.1-x86_64-pc-windows-msvc'
just dev
```

This repository is private; cloning requires access to it. Both peers run their
own source checkout. Open settings from the tray, select **Together**, create or
join a room, then enable the sharing you want. Invitations expire for first join
after ten minutes. Sharing returns to off after reconnection.

Development data is separate from an installed production BongoCat. A locally
built executable depends on workspace resources; it is not a standalone portable package.
See the [full guide](docs/bongocat-together-usage.md) for connection, model and sharing controls.

## Make room for your friend's cat

Choose a local model, adjust its scale, hide it, swap cat positions or reset the
layout. Available built-in models can match automatically. Custom models must be
imported locally on each computer; model files are not transferred over the connection.

Browse community models at [Awesome-BongoCat](https://github.com/ayangweb/Awesome-BongoCat)
and respect each model's license. Wheel direction appears in the bubble; dedicated
wheel or double-click animations depend on the model.

## What has been checked

Windows automated checks cover real local QUIC connections, permission revocation,
independent cat runtimes, model switching and failure recovery, text bubbles,
disconnect cleanup, and the Together settings layout at 800 × 600 and 1000 × 600.

```powershell
just check
just together-smoke
just dev-smoke
```

**Still awaiting real-world acceptance:** two computers on different public networks,
relay and network changes, Chinese IME composition, physical input and lock/sleep/UAC,
125/150/200% DPI and multiple displays, and an eight-hour stability run.

Friend display preferences and application allowlists currently last only until
the app exits. Bubble styling follows the app theme. Together is not implemented
on macOS; some other-language Together labels currently use English. Signed
installation, upgrade/uninstall testing and redistribution checks remain pending.

## Credits and license

Based on [ayangweb/BongoCat](https://github.com/ayangweb/BongoCat), with inspiration
from [MMmmmoko/Bongo-Cat-Mver](https://github.com/MMmmmoko/Bongo-Cat-Mver).
Upstream history and license notices are retained. Upstream releases do not contain
this Together development work.

Application code uses [Apache License 2.0](LICENSE). Live2D Cubism Core and model
assets retain their own terms; see the bundled [Cubism license](vendor/cubism/5-r.5/LICENSE.md)
and [notices](vendor/cubism/5-r.5/NOTICE.md).

For changes, read the [contributing guide](CONTRIBUTING.md).
