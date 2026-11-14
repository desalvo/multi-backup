# Multi Backup r021 — certification and release acceptance matrix

**Audit date:** 2026-11-14. **Source:** draft PR #9, branch
`feature/r019-rust-native-encrypted-archive`.
**Disposition: NOT CERTIFIED. No stable release permitted.**

This file is a machine-auditable checklist of required evidence; a green
compilation, local test suite, or prototype Docker image does not certify the
complete backup appliance. Keep the working r018 on main untouched.

| Gate | Requirement for certification | Verified evidence/status |
| --- | --- | --- |
| Production gate | Python/Rust tests, Ruff/Bandit, dependency audits | PASS on 43606951658; repeat on latest head |
| Storage v1 | Dedup, age, parity, fault injection, verify and restore | Code/test added; not formally qualified |
| Portable ZIP64 | Integrity-checked multi-volume export/import and retry | Code/test added; needs large-volume qualification |
| Logical browse | GitHub, LFS, CI, metadata, Dropbox under ONE backup | Rust manifest and responsive web prototype; provider E2E pending |
| Complete native Rust | No Python runtime in installer or provider actions | Rust preview built in prior revisions; complete parity not proven |
| Desktop platform CI | Mac ARM/Intel, Windows, EL8/9/10, Ubuntu | FAIL on 43606951658; follow-up build pending |
| Production app signature | Apple Developer ID + notarization, Windows Authenticode, signed RPM repo | Credentials/hardware validation unavailable |
| Registry | Multi-arch Docker Hub with SBOM/provenance, architecture runtime tests | OCI build works, push unverified/not configured |
| Kubernetes | Helm/Kustomize render, PVC, TLS, NGINX/HAProxy, wizard, upgrades, backup persistence | Config added; deployment and upgrade acceptance pending |
| Identity/auth | Local + LDAPS + OIDC SSO, roles, account lockout, TLS | Argon2id local prototype; LDAP/OIDC absent |
| SMTP(S) and audit | TLS certificate check, immutable audit, retention, retries | Minimal JSONL audit; SMTP absent |
| Desktop mount | Read-only FUSE/WinFsp mount with on-demand chunk reads, unmount and symlink controls | Not implemented |
| Storage conversion | Old tar.gz.age ⇄ content-addressed, in-place storage move with rollback | Not implemented |
| Packaging | One readable logical backup; original branded images; manuals/photos | Original artwork source, PDF/manual regeneration; review pending |
| Live provider acceptance | Real GitHub/Dropbox backup & restore, rate limit, interruption, 5 snapshots | Not run; private credentials unavailable |
| Durability | Independent full restore, no data loss during crash, parity corruption, key loss drills | Needs long-running test/certification |
| Security | Threat modeling, SAST/SCA, secret handling, penetration test, restore sandbox | Initial gates; independent assessment pending |

## Publication interlocks

- Any PR: tests, previews and OCI build only; **never** registry push.
- Early on main + `[full-ci]`: release only after **all** native Rust
  previews, server, security, and publication jobs pass.
- Stable tag: requires `ENABLE_STABLE_RUST=true` explicitly **in addition**
  to the signing and native gate. Do not configure it prematurely.
- Docker stable additionally requires `ENABLE_STABLE_CONTAINER=true`.
- Docker Hub needs GitHub Actions `DOCKERHUB_USERNAME=desalvo` (variable),
  `DOCKERHUB_TOKEN` (secret). Do not include tokens in source, logs or chat.

## Acceptance evidence to attach

Record exact Git commit SHA and GitHub Actions run/job URLs, OS versions,
SHA-256 hashes of installers, notarization/ticket and Authenticode outcomes,
a verified roundtrip restore digest, corruption/recovery results, package
manifest SBOM and provenance. Complete the checklist above only when each
external requirement is actually performed and independently observed.

### Critical note

A *single logical backup* contains all applicable repositories, LFS,
CI artifacts, metadata and selected Dropbox content. Chunk and stripe
objects are storage internals, **never separately presented as user backups**.

Do not treat signing/build compliance as equivalent to full backup
recoverability or an external security certification.


## r022 newly implemented and pending external qualification

- Read-only health verification of every encrypted chunk plus parity checks.
- ZIP64 volumes of encrypted content-addressed objects, with encrypted catalog and SHA-256 validation before atomic import.
- Non-destructive streaming import of multiple legacy age/tar archives into a **single** logical backup.
- Multi-selection restore and responsive file-manager-like web browse.

These features require full post-merge qualification before marking them production certified. Reverse conversion, production native signing, live mount, OIDC/LDAPS and SMTP remain explicit release blockers.


## r023 outstanding integration certification

The native Rust storage pane, relocated store, SMTP(S), LDAPS and OIDC
backends have been implemented as development code. Their presence is
not certification. Validate latest CI, OIDC issuer/JWKS rotation/nonce,
LDAP directory group membership and TLS, SMTP actual delivery and
certificate rejection, external identity authorization and audit
before enabling remote production access.

Live filesystem mounts, in-memory age session interoperability and
direct provider-to-chunk ingestion remain absent; stable promotion is blocked.


### r024 experimental mount

WebDAV read-only mount endpoint implemented as development code, with streaming verified encrypted-chunk access and separate Basic-auth token. **Unverified against OS clients; security and restore acceptance remains open.** Native FUSE/WinFsp mount is still absent. Do not enable auth.dav or treat it as a production mount until verified.


## r025 conversion acceptance

The code path for v1 -> legacy archive export (streaming) and legacy -> v1 import is implemented and has a round-trip test. **Certification pending** on actual GitHub, Dropbox, LFS and large datasets. Do not delete original archives until independent restore and checksum validation pass.


## r027 -- Debian and private architecture qualification

Native Debian 12 Bookworm and Debian 13 Trixie x AMD64/ARM64 must each
pass cargo compile, independent .deb dependency resolution, installation,
native CLI version, GUI self-test, byte-identical extraction and SHA-256.
Their release outputs must include all four .deb files.

Private design confidentiality: verify a password-chosen AES-256 R6 PDF
is locally generated without intermediate plaintext files, rejects incorrect
passwords, is never disclosed in GitHub Actions, and only encrypted bytes
are optionally added. The public repository must contain **no private
diagram source or live topology**. This is a process safeguard, not
an international security certification or resistance guarantee.
