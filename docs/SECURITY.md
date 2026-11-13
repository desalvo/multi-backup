# Multi Backup - security engineering and AGID-aligned practices

**Target:** defense-in-depth, minimization, traceability and repeatable validation consistent with secure public-sector software development practices. This document is a design and test baseline, **not a certification of AGID compliance**.

## Security boundaries

1. GitHub credentials remain in GitHub CLI's credential store. Dropbox token is accepted from the environment or OS credential manager, not from command-line arguments or plaintext settings.
2. Private encryption identities live outside ciphertext backup storage and are created with owner-only permissions. The app stores only public age recipients in settings.
3. Archive paths are validated against traversal, symlink and tar device entries. Encrypted output is written as `.partial`, finalized with atomic rename and verified by independent SHA-256 sidecars.
4. Failed or interrupted runs preserve checkpoints and verified archives; old backups are not overwritten by `--restart`. Migration of monolithic archives keeps the source immutable until split archives pass integrity checks.
5. CI artifact deletion is disabled by default. When selected, the engine scans the current GitHub inventory, protects live and unclassified dependencies, offers dry run, reports per-repository progress and totals. Third-party unreported dependencies cannot be proven safe to delete.
6. Dropbox backend calls only read-only API endpoints. It has no purge or delete methods, and it does not delete expired historical content already stored locally.
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
