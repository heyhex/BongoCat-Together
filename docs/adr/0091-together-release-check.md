# ADR-0091: Together release checks

Status: accepted
Date: 2026-10-15

## Decision

Together checks the public `heyhex/BongoCat-Together` GitHub Releases API,
including prereleases. Only `together-v<semver>` tags participate in version
comparison; drafts, upstream tags and dated test tags do not. The newest version
must carry the exact Windows x64 Together installer with a matching HTTPS GitHub
asset URL. A missing installer, invalid response or network error is a failure,
never an up-to-date result. Requests are anonymous, bounded in time, body size
and pagination, with no redirects, credentials or third-party proxy fallback.

Reuse the existing worker, diagnostics, automatic-check preference, update
window and external-link worker. Release information explicitly distinguishes
browser downloads from signed installation; both the UI and worker enforce it.
Read-only checks are safe in Development and Production: neither replaces the
installation nor accesses the other environment's data. No persisted fields change.

This is a version notification with a Release-page link, not automatic installation.
Together does not inherit upstream's signing key. The signed update runtime stays
fail-closed until Together provisions its own key and validates installation and
rollback. Packaging metadata and runtime repository identity point to Together. Windows
installer filenames use `BongoCat-Together_<version>_x64.exe`; packaging and CI
retain this existing published convention so later releases remain discoverable.

The implementation reuses the pinned cargo-packager-updater HTTP/semver exports
and existing workspace serde/serde_json versions. No new library or platform API
is introduced; transport and GitHub DTOs stay private to bongocat-update.

## Acceptance

Cover prereleases, semver ordering, invalid/dated/upstream tags, drafts, foreign
URLs, missing assets, malformed/oversized responses and transport failures.
Verify read-only results offer the Release link without an install button and
reject an Install command. Run anonymous live checking against the public repo,
workspace checks and the Windows settings smoke. macOS Together delivery and
automatic signed installation remain outside this increment.
