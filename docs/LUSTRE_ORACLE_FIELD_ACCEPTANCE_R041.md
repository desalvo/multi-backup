# r041 — Lustre / Oracle field acceptance (15 November 2026)

## What is actually implemented

This is an **acceptance and safety** change stacked on draft PR #12 `feature/r040-real-kubernetes-ci`. It does not merge PR #10/#11/#12 or publish stable packages. A branch with a passing hosted safety suite is **not** field-certified.

| Provider | Rust capability at r041 | Hosted CI | Real field certification |
|---|---|---|---|
| Lustre | Generic Rust encrypted full/incremental read from an **exact RO Lustre mount** with operator snapshot attestation; create-only restore to an **exact RW Lustre mount** | Fake mount, fake snapshot and false restore target are rejected | **PENDING** actual Lustre MDT/OST cluster, consistent snapshots, restore, metadata and stripe fidelity |
| Oracle | Declarative planned plugin. No Oracle RMAN backup-piece capture or restore adapter in Multi Backup | Oracle cannot be selected via the generic database adapter; plans remain non-executable | **PENDING** licensed isolated DB. RMAN validation probes are prepared, but DO NOT constitute a Multi Backup backup/restore |

## Hosted checks (safe on GitHub-hosted CI)

Workflow: `.github/workflows/lustre-oracle-guards.yml`, including `bash -n` and the **compiled Rust** CLI via `scripts/test-lustre-oracle-guards.sh`.

- Real encrypted POSIX control snapshot, then fail-closed Lustre `plugins probe`, `plugins backup`, `plugins restore` against ordinary local directories.
- Oracle catalog `plugins plan` remains non-executable in both directions; a fake `engine=oracle` profile cannot sneak through the PostgreSQL/MySQL adapter.
- Both field scripts must reject missing explicit approval before touching hardware.
- These tests do **not** emulate Lustre or Oracle. They are negative/safety checks only.

## Real Lustre field fixture

Workflow: `.github/workflows/lustre-oracle-field.yml` -> `lustre`, or invoke `bash scripts/test-real-lustre-roundtrip.sh` directly on an approved disposable node.

Prerequisites:

1. A genuine authorized Lustre deployment with healthy MGS/MDS/OSS and sufficient OSTs; a dedicated, nonproduction test namespace. Install the actual compatible Lustre client, `lfs`, `age-keygen`, `jq`, `getfacl`, `getfattr`, Rust compiler. Do not install unsigned client/kernel modules on production.
2. **Two different already-created consistent MDT+OST filesystem snapshots**, mounted as **two separate exact RO Lustre mountpoints**. These must be full and incremental **filesystem states**, not mere directory copies or bind mounts. The operator must independently verify snapshot consistency.
3. Seed the dataset on a disposable writable lab volume **before taking snapshots**: in the full state create `.multi-backup-ci-fixture` containing exactly `DEDICATED_LUSTRE_TEST_DATA_V1`, `README.txt`, `payload/removed.bin`, and `payload/stripe.bin` with a deliberate nondefault mode, test ACL, `user.multi_backup_ci` xattr, fixed mtime and nondefault Lustre stripe count. Then change `README.txt`, delete `payload/removed.bin`, add `payload/new.bin`, and retain a `payload/stripe.bin` with the same controlled metadata in the incremental state; preserve the fixture marker. Use your Lustre cluster's supported, coordinated snapshot procedure.
4. A **third separate exact RW Lustre mountpoint** is reserved exclusively for **new** output directories. The acceptance script never overwrites an existing restore target and **leaves its two recovered directories on that mount for inspection**. Do not point this at production storage.
5. Register one dedicated, owner-approved GitHub ephemeral runner with labels `self-hosted,linux,x64,lustre-ci`, create the root-owned marker `/etc/multi-backup-ci/allow-disposable-lustre` containing exactly `DEDICATED_ISOLATED_LUSTRE_FIELD_LAB`; configure environment `lustre-field-lab` with mandatory reviewer approval. In environment vars set `LUSTRE_FULL_MOUNT`, `LUSTRE_INCREMENTAL_MOUNT`, `LUSTRE_RESTORE_MOUNT` and `LUSTRE_SNAPSHOT_ATTESTED=COORDINATED_MDT_OST_LAB_SNAPSHOTS`.

The script checks the actual Linux `/proc/self/mountinfo` through the compiled Rust `plugins probe`, including Lustre filesystem type, exact mountpoint and RO/RW flags. It exercises age-encrypted full and incremental archives, backup and chain verification, byte-for-byte comparison, deletion propagation, and refusal to overwrite. It then strictly compares **file mode, numeric owner/group, mtime, ACL, xattr and Lustre stripe count**. Missing attributes or command failures are FAIL, never silently skipped.

**Known expected limitation:** the current generic Rust file-byte archive does not attest complete POSIX ACL, user xattr, ownership, mtime or Lustre stripe metadata restoration. Therefore the field suite may pass content but correctly **FAIL** metadata fidelity. A red field test is evidence of an outstanding implementation gap, not a reason to remove those assertions.

## Licensed Oracle RMAN lab-only validation

Workflow: `.github/workflows/lustre-oracle-field.yml` -> `oracle`, or `bash scripts/test-oracle-rman-lab.sh` on the approved isolated licensed database host. **No Oracle binaries, passwords, software images or database contents are added to this repository or its CI artifacts.**

1. Provision a licensed/suitable **isolated disposable Oracle** instance (mounted/open, primary, dedicated for tests), with existing valid RMAN backup pieces accessible for `RESTORE DATABASE VALIDATE`. Authorize local operating-system authentication `rman target / nocatalog` and `sqlplus -L -s "/ as sysdba"` to the dedicated nonroot test account. Verify license entitlements independently.
2. Register an isolated owner-approved runner labeled `self-hosted,linux,x64,oracle-ci`, protected with environment `oracle-field-lab`. The root-owned marker `/etc/multi-backup-ci/allow-disposable-oracle` must contain `DEDICATED_LICENSED_ORACLE_LAB_ONLY`.
3. Configure environment vars `ORACLE_HOME`, `ORACLE_SID`, `MB_ORACLE_EXPECTED_DB_NAME`; the workflow passes `MB_ORACLE_LAB_APPROVAL=LICENSED_DISPOSABLE_ORACLE_LAB`. The SQL preflight refuses a mismatching DB name, non-primary role, or non-READ WRITE state before RMAN.
4. The test executes **`BACKUP VALIDATE CHECK LOGICAL DATABASE;`** and **`RESTORE DATABASE VALIDATE;`** with local OS authentication, no application credentials in GitHub, and checks for failure codes and independent completion signals. It does **not** run `BACKUP DATABASE`, write RMAN backup pieces, restore a database, or encrypt an Oracle backup through Multi Backup. That requires a future native Oracle adapter and an isolated genuine new-target recovery test.

Note that Oracle's `BACKUP VALIDATE` reads source files without creating an RMAN backup, and `RESTORE ... VALIDATE` reads existing backups without restoring files. They check prerequisite viability, **not** full backup/restore certification. See [Oracle validation documentation](https://docs.oracle.com/en/database/oracle/oracle-database/19/bradv/validating-database-files-backups.html).

## Safe manual dispatch and evidence

`.github/workflows/lustre-oracle-field.yml` is `workflow_dispatch` **only**, with no PR/push triggers; a GitHub-hosted approval step checks the actor `desalvo`, exact 40-digit source SHA, provider, and literal confirmation `APPROVED-DISPOSABLE-FIELD-LAB` **before** any self-hosted runner is assigned. The self-hosted job then uses an exact SHA checkout, confirms `git rev-parse HEAD`, checks lab markers and environment approval, and runs the real field script.

As with other manual workflows, the workflow must exist on the **default branch** to be dispatchable from the Actions UI. Until the workflow is reviewed and available there, the branch only contains prepared procedures. Do not merge the stacked development PRs to unblock it without approval. Do not reuse a runner exposed to unreviewed PR code.

## Remaining RC blockers

- Lustre: actual field run, snapshot coordinator proof, POSIX and stripe metadata completeness, interrupted copy/restore and cross-OST failure tests.
- Oracle: separately licensed operator lab, implemented Rust RMAN backup-piece capture with durable encrypted transfer and recoverable logs/controlfile/SPFILE, full and incremental recovery into a **new isolated** instance, credential/secret hygiene, PITR and failure injection. RMAN read validation alone is **not enough**.
- Global: exact-head Kubernetes 2-pod TLS and failure gate, genuine shared-store quorum/fencing, AFS field certification, agents, signed installers, independent security audit. **Keep stable blocked.**
