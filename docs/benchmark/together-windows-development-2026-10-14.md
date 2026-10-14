# Together Windows development verification — 2026-10-14

Scope: local Windows x64 MSVC Development build, existing release profile unchanged.
These resource measurements precede the Together settings layout correction. The recorded SHA-256 identifies
the measured build and differs from subsequent local rebuilds.
Source baseline: upstream v2.3.1 plus uncommitted Together changes. This is not release acceptance.

## Functional gates

- `RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-msvc`, `CARGO_BUILD_JOBS=2`, `just check`: passed.
  1,196 tests passed, zero failed; five default-ignored entries. Evidence: `target/together-check-final.log`.
- `just together-smoke`: passed real encrypted direct loopback, two native Live2D overlays, peer model hints/fallback,
  manual model switch, invalid-model rollback and retry, scale/visibility/position swaps, isolated remote keys,
  Unicode bubble, indicator-failure revoke and disconnect cleanup. Evidence: `target/together-native-smoke.log`.
- `just dev-smoke`: final optimized build and original settings/offline runtime smoke passed, exit 0.
  Evidence: `target/together-offline-smoke-final.log`.
- Final optimized exe launched via Start-Process with `--together-smoke`, waited for completion: exit 0.
- Official cargo-deny 0.20.2: licenses and sources passed unchanged deny.toml; checksum/source recorded in the phase plan.
- Eight locale catalogs with 329 keys each validated. Targeted changelog/packaging/version/repository-cfg contracts: 51 passed.
- Broader Python suite has one independently reproduced upstream Windows path-format failure; 101 others passed.

## Bounded resource observation

Executed the final optimized exe directly with `--run-seconds 20`, hidden launcher, normal native model window,
settings closed and no room. Sampled process resources every 500 ms; startup is included.
This measures one local cat, not two cats or a connected-network workload.

| Measurement | Observed |
| --- | --- |
| Process exit | 0 |
| Wall time | 20.94 seconds |
| Samples | 40 |
| CPU time through last sample | 4.656 seconds |
| Average CPU, one-core basis | 22.79% |
| Logical processors | 20 |
| Average CPU, whole-machine basis | 1.14% |
| Peak working set | 123.16 MiB |
| Peak private bytes | 252.8 MiB |
| First / final sampled handles | 628 / 1000 |
| Final sampled threads | 111 |

The handle count grew during this startup-inclusive interval; this observation does not establish steady-state
behavior or diagnose a leak. No baseline comparison, GPU-memory measurement, network-traffic measurement,
long-running trend or eight-hour stability result is claimed. Next resource work is a warm-up-separated,
connected dual-cat soak with CPU/memory/GPU/handles/threads/traffic tracked over time.

Raw evidence: `target/together-resource-sample.json`. The executable is
`target/release/bongocat-app.exe`, SHA-256 `4d7b9671c919336f24557e7ed919cfb5b9830fe50dc12c450f45299fd86ad5c8`.
It depends on this checkout's model resources and is not a standalone installer.

## External acceptance still required

Different public networks and relay routing; actual network changes; physical keyboard/mouse and
lock/sleep/UAC/permission transitions; Chinese IME composition; 125/150/200% DPI and multiple displays;
custom-model UI acceptance; long-running resources; Cubism redistribution, signing and installer lifecycle gates.
