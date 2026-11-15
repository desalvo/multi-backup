# Multi Backup — r038 documentation index and coverage audit

**Audit date:** 2026-11-14. **Current engineering branch:** PR #10,
`feature/r038-rust-primary-plugin-executor`. Status: **Rust development
preview; not a published RC**. This page is the point of entry for users,
release reviewers and future development handoff.

## Current documents by audience

| Audience | Primary source | Coverage / authority |
| --- | --- | --- |
| Italian operator | [OPERATIONS_R038_IT.md](OPERATIONS_R038_IT.md) | Rust CLI workflows, restore, schedule, full/incremental, purge, GC/repack, provider account setup |
| English operator | [OPERATIONS_R038_EN.md](OPERATIONS_R038_EN.md) | Same operational contract, English |
| Product/release reviewer | [FEATURE_INVENTORY_R038.md](FEATURE_INVENTORY_R038.md) | Implemented vs Preview vs Planned vs Blocked, explicit certification gaps |
| Storage engineer | [CHUNK_STORAGE_V1.md](CHUNK_STORAGE_V1.md) | age+zstd/HMAC, full/incremental manifest, 8+2 parity and crash-safe repack |
| Maintenance operator | [MAINTENANCE_POLICY.md](MAINTENANCE_POLICY.md) | Scheduling/verification, safe purge, GC, protection and 48h repack grace |
| Plugin developer | [PLUGIN_API.md](PLUGIN_API.md), [PLUGIN_MATRIX.md](PLUGIN_MATRIX.md) | Directional provider contracts, real executor scope, source vs destination account authorization |
| Security reviewer | [SECURITY.md](SECURITY.md), [BACKUP_PROTECTION.md](BACKUP_PROTECTION.md) | Key boundaries, holds, fail-closed operations, independent WORM |
| Installer/release | [RELEASE_CHANNELS.md](RELEASE_CHANNELS.md), [RC_CERTIFICATION_MATRIX.md](RC_CERTIFICATION_MATRIX.md) | Native packaging, early/stable separation, release-blocking evidence |
| Future development | [DEVELOPMENT_STATUS.md](DEVELOPMENT_STATUS.md), [HANDOFF.md](HANDOFF.md), [RUST_PORT_INVENTORY.md](RUST_PORT_INVENTORY.md) | Revision timeline, open tasks, source code and continuation context |

## Full r038 operational coverage checklist

- [x] Rust native startup, key setup, v1 store and data safety
- [x] Manual full and incremental snapshots and immutable chunk deduplication
- [x] Four requested file/byte statistics and exact full/incremental lineage
- [x] Verified full-chain-by-default and selective new-directory restore
- [x] User confirmation, protected successor checks and chain-preserving purge
- [x] Separate GC preview/execute and delayed physical reclaim
- [x] Automatic mixed 8+2 parity stripe repack; encrypted successor index published last
- [x] Crash-before-index orphan parity cleanup and post-commit grace-period description
- [x] Source account vs independently authorized destination account
- [x] GitHub full repo and Dropbox one-file cross-account limitations
- [x] Manual/scheduled Rust actions and scheduler configuration example
- [x] Current plugin registry vs executed/production-certified role distinction
- [x] Explicit external proof gaps and real-data recovery requirements
- [x] User decision: old Python archive conversion is optional
- [ ] Signed/notarized public RC and actual live-provider/tested install/rollback evidence
- [ ] Cross-provider generic writer and authenticated partial remote restore
- [ ] Persisted native GUI schedule editor and dynamic verify-latest backup selector
- [ ] Crash-fault-injection qualification of repack across all OS/filesystem targets

Checklist checkmarks in this section mean **documentation present**, not
production certification.

## Legacy document separation

The PDF manuals/brochures under `docs/manuals/` and `docs/brochures/`
originated from the **r018 Python/Tk** interface and may show Tk screenshots.
They must not be described as screenshots of the Rust GUI. The publication
generator now appends the r038 Rust operational procedures and labels the
historical screenshots as such. The canonical r038 instructions remain the
version-controlled Markdown manuals above until the generated PDF outputs
have been rebuilt and independently inspected.

Historical r010-r037 sections remain in the individual engineering files to
preserve development provenance. **The most recent r038 section at the top
or end of each file overrides older status statements**. A green PR #9 CI
does not qualify later PR #10 functional changes.

## Documentation maintenance contract

For any new command, setting, behavior or security rule:
1. Update the affected Rust code and acceptance test.
2. Update both `OPERATIONS_R038_IT.md` and `OPERATIONS_R038_EN.md`
   with executable examples, defaults, limitations and failure behavior.
3. Update `FEATURE_INVENTORY_R038.md` and the relevant engineering reference.
4. Update `DEVELOPMENT_STATUS.md`, `HANDOFF.md` and the RC matrix, including
   the exact CI run / SHA only after results are actually available.
5. Regenerate bilingual PDF manuals/brochures from
   `scripts/build-publications.py`; distinguish legacy Tk screenshots from
   Rust native screenshot evidence. Inspect text and rendered pages.
6. Run `python scripts/check-documentation-consistency.py` inside the
   Production Gate; do not merge if any required operational coverage is lost.
7. Do not claim signed RC, fully migrated plugin roles, WORM, live backup
   certification or physical user installation without independent proof.
