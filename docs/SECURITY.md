# Multi Backup - security engineering and AGID-aligned practices

**Target:** defense-in-depth, minimization, traceability and repeatable validation consistent with secure public-sector software development practices. This document is a design and test baseline, **not a certification of AGID compliance**.

## Security boundaries

1. GitHub credentials remain in GitHub CLI's credential store. Dropbox token is accepted from the environment or OS credential manager, not from command-line arguments or plaintext settings.
2. Private encryption identities live outside ciphertext backup storage and are created with owner-only permissions. The app stores only public age recipients in settings.
3. Archive paths are validated against traversal, symlink and tar device entries. Encrypted output is written as `.partial`, finalized with atomic rename and verified by independent SHA-256 sidecars.
4. Failed or interrupted runs preserve checkpoints and verified archives; old backups are not overwritten by `--restart`. Migration of monolithic archives keeps the source immutable until split archives pass integrity checks.
5. CI artifact deletion is disabled by default. When selected, the engine scans the current GitHub inventory, protects live and unclassified dependencies, offers dry run, reports per-repository progress and totals. Third-party unreported dependencies cannot be proven safe to delete.
6. Dropbox **backup reading** calls only read-only API endpoints and does not delete source content. Separately, an explicitly confirmed restore may upload a NEW Dropbox file using a destination-specific write token. The Rust restore verifies that token's target account and refuses overwrites. Live cross-user testing is still required.
7. Network update requests require HTTPS and an allowlisted release asset domain; download requires a supplied GitHub SHA-256 digest. The app does not silently execute installers. Production auto-install needs platform signing and explicit secure policy.
8. Native releases require least-privilege CI permissions, artifact integrity, dependency scanning and signed runtime packaging; currently missing signature secrets are release blockers for unattended updates.

## Threats and tests

- **Path injection:** no archive absolute or `..` members; no symlinks on extraction; key names validated against a strict regex.
- **Credential leakage:** token values must not appear in CLI logs, backup metadata or packaged files. Avoid passing them through shell history. Environment variables are for local interactive use; OS keyring is recommended.
- **Artifact loss:** GitHub deletion is not reversible. Use simulation and review release dependencies. Archive checksums do not substitute recovery drills.
- **Supply-chain compromise:** GitHub Actions declared with minimal permissions, but third-party actions still need pinning to immutable SHAs before production certification; installers should be notarized/signed. Enforce branch protection and secret scanning on the repository.
- **Disk theft:** encryption uses age, but a compromised workstation with an unlocked private key can decrypt archives. Keep offline identities.
- **Update tampering:** SHA-256 integrity detects transfer corruption, but signatures bind publisher identity. Until signing is configured, users must review downloaded installers manually.

## Validation commands

```
python -m compileall -q .
python -m unittest discover -s tests -v
python multi_backup.py --version
python multi_backup.py plugins
```

Security-related changes should have regression tests. Vulnerabilities should be privately reported through GitHub security advisories rather than public issues.

## R013: protected identity vault, biometrics, TLS and remote restores

The age private key vault is an authenticated scrypt (N=32768, r=8, p=1)/AES-256-GCM envelope under per-user Application Support, separate from the app bundle. A fresh identity is generated into process memory; older plaintext keys are encrypted, verified and removed without rotation. A strong application password unlocks once per session. The in-memory key is temporarily written to a mode-0600 file only while an age subprocess needs a filename, then removed. Enable OS full-disk encryption to protect deleted legacy disk blocks and live temporary files. A recovery copy uses a separate passphrase-derived salt/key and AES-GCM nonce; export verifies the original application password and refuses overwrites. Application upgrades leave the vault untouched.

On macOS, opt-in Touch ID uses a helper with Security.framework device-only Keychain `SecAccessControlCreateWithFlags(..., .biometryCurrentSet)` protection and LocalAuthentication. Biometric templates and Mac login passwords are never handled by Multi Backup. Biometric enrolment changes may invalidate the item; password fallback always works. Unsupported platforms do not claim biometric support.

Updater HTTPS uses trusted certifi CA roots with server certificate chain and hostname checking enabled. GitHub release downloads require a trusted SHA-256 digest, and HTTP redirects remain allowlisted. No insecure SSL verification bypass is accepted. Restore checks the source archive's plugin, never auto-overwrites an existing remote target, and prompts for the service-specific credential only when required. Dropbox write tokens are not saved. Full live-account and platform biometric review remains outstanding.


## Confidential design material

This repository is public. Never commit live vault topology, recovery
phrases, IPs, names, secret files, private diagrams, passphrases or
unencrypted architecture documents. The separate offline design kit
can produce an AES-256 R6 PDF using an operator-chosen local password.
Only the strongly encrypted PDF is allowed under docs/private, checked
by scripts/check-confidential-artifacts.py in the Production Gate.
A private repository remains preferable even for ciphertext because
Git objects are widely copyable and password guessing can be attempted
offline. Never share the PDF password in chat, commits, CLI arguments or CI.


## r038 — additional native safeguards and unqualified boundaries

- The source account of a provider snapshot is historical provenance, not write authority. Restore into a different user requires independent destination credentials and explicit target selection; current GitHub new-repository and Dropbox new-file routes are narrow previews.
- Full/incremental manifests and chunk references are age authenticated; the *multi-manifest* child-rebase path still needs forced-power-loss/fault-injection qualification before production use.
- Logical purge changes/manages manifests only and does not reclaim ciphertext. A separate, operator-confirmed local GC uses a common store lock, verifies all surviving backup chains and parity, and applies a 48-hour default orphan grace (minimum 24). Mixed live/dead parity stripes are deliberately retained until crash-safe repacking works.
- The persistent-state scheduler is in development. Keep JSON/state files private and secret-free; OS service isolation, monitoring, retention legal holds, independent WORM and remote GC are NOT certified.
- [Runbook IT](R038_OPERATIONS_IT.md), [runbook EN](R038_OPERATIONS_EN.md), [release gates](RC_CERTIFICATION_MATRIX.md).

## r038 — Sicurezza di restore tra utenti, purge e repack automatico

Le credenziali del proprietario originale sono soltanto provenienza, non autorizzazione a scrivere nel nuovo account. Il restore GitHub verifica lo slug del nuovo owner e richiede gh autorizzato; Dropbox può verificare l'account del token WRITE della destinazione. Il purge modifica solo manifest cifrati e blocca successori protetti. La GC esamina tutti i backup sotto lock, esegue automaticamente il repack delle stripe 8+2 miste preparando e verificando nuovi shard prima del commit dell'indice autenticato, conserva la vecchia parità fino al grace del successore e rimuove soltanto ciphertext orfano dopo conferma. Non è WORM indipendente; restano da certificare crash, multiwriter e comportamento remoto.

