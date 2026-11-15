# Multi Backup r039 — Native agents and OpenAFS operations (development preview)

**Status:** native Rust source and platform packages under development. NOT a certified RC. The agent service is not automatically enabled; registration, explicit server approval, TLS, security and live recovery checks are mandatory.

## AFS execution models

AFS backup is always performed with **specific volume names/IDs**, using the separately installed OpenAFS vos utilities, not a generic POSIX directory backup.

- Local host: vos dump with an existing authorized AFS token, or explicit keytab/localauth mode subject to separate privileged credential handling.
- Agentless remote SSH: authorized SSH key, independently pinned host key, strict host verification, no SSH agent forwarding. Remote AFS authentication is separate from SSH, e.g., existing token, remote keytab or localauth on an approved fileserver.
- Managed agent pull: the server requests a preapproved profile; the agent polls outbound over HTTPS and runs that profile only when it is powered on and its local hash matches the approved profile.
- Managed agent push: the agent independently selects a local approved profile, runs the backup, streams the cleartext dump only through the age encryption process and sends the ciphertext over authenticated HTTPS.
- Other plugins/scripts: the process is designed to be extensible. Untrusted server-supplied command text is never executed as a shell expression. Locally edited command profiles require explicit administrative review and signing before dispatch.

OpenAFS dump full: vos dump -id VOLUME -cell CELL. Incremental: vos dump -id VOLUME -time "mm/dd/yyyy HH:MM", with a valid gap-free predecessor dump timestamp. AFS restore: vos restore to an approved fresh volume, then apply incremental dumps in chronological order. The real server-side AFS restore and ACL/VLDB acceptance remain a release gate.

## Agent identity and security

Each agent has a unique ID, linked user, separately provisioned high-entropy token and Ed25519 device key. Every request signs method, route, ciphertext hash, metadata hash, monotonic sequence and timestamp. Server-side acceptance must check registry association, explicit admin approval, revocation, signature and replay counter persisted across restarts. Only certificate-verified HTTPS may be used. The agent has NO inbound listener. Device private key and token are private files, not command-line flags, URLs or plaintext backup metadata.

Example setup on Linux/macOS:

~~~bash
mkdir -p "$HOME/.config/multi-backup-agent"
chmod 700 "$HOME/.config/multi-backup-agent"
multi-backup-agent init --id afs-host-01 --user backup-operator \
  --server https://backup.example.org/ \
  --config "$HOME/.config/multi-backup-agent/agent.json"
multi-backup-agent status --config "$HOME/.config/multi-backup-agent/agent.json"
~~~

The output contains ONLY the public device key for registration. An admin approves its identity out of band and provisions the token into agent-server-token.txt (0600). Installation must never auto-approve devices or silently start a service.

Example locally approved profile at profiles/volume-test.json:

~~~json
{
 "id":"volume-test",
 "recipient":"age1REPLACE_WITH_PUBLIC_RECIPIENT",
 "enable_push":true,
 "job":{
  "id":"volume-test","cell":"example.org","volume":"user.test",
  "auth":{"type":"existing_token"},
  "execution":{"type":"managed_agent","agent_id":"afs-host-01"},
  "mode":{"kind":"full"},"server":null,"partition":null,
  "command_profile":null
 }
}
~~~

~~~bash
multi-backup-agent preview --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent run --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent push --config ~/.config/multi-backup-agent/agent.json --profile volume-test
multi-backup-agent poll --config ~/.config/multi-backup-agent/agent.json --once true
~~~

Backup payloads are age-encrypted before touching the local staging area; on a failed upload the encrypted spool file is retained for investigation. AFS tools vos, kinit and aklog must be installed/authorized on the originating host separately. Privileged localauth must be explicitly opted in by a trusted administrator, never enabled as the default daemon account.

## Installation matrix

| OS | Architecture | Native installer |
| --- | --- | --- |
| macOS 13+ | arm64 / x86_64 | signed-in-future native PKG, manual LaunchAgent template |
| Windows | x64 | Inno Setup EXE, opt-in Windows Task Scheduler service account |
| Debian 12 / 13 | amd64 / arm64 | DEB with disabled systemd unit |
| Rocky Linux 8 / 9 / 10 | x86_64 | RPM with disabled systemd unit |
| AlmaLinux 8 / 9 / 10 | x86_64 | RPM with disabled systemd unit |

No Python, Tk, PyInstaller or arbitrary remote execution. Installers are preview artifacts until tested on every target architecture; signing, notarization and rollback remain outstanding.

## Required functional and security qualification

Full local/SSH/agent AFS dump and new-volume restore; incremental AFS chain time gap detection; keytab/localauth privileged tests; TLS certificates and MITM detection; correct server-side user-token registration, approval and revocation; Ed25519 spoofing, replay and clock skew; profile tampering; agent shutdown/resume, offline server and protected encrypted spool; live push/pull with auditable ID and authenticated status; native OS installs and upgrades; service accounts; scripts/command allowlist; full byte-for-byte restore and independent security review. Do not promote the RC until these are complete.

## r039 — parallel backup execution and host budgets

Agent local config has limits; server policy is set by authenticated admin POST /api/agents/admin/limits. Pull obtains limits from the approved agent poll; autonomous push first requests GET /api/agents/policy. The stricter of server and agent budgets applies. max_concurrent_jobs=1..256, threads_per_job=1..64, optional bytes/sec per stream, CPU/RAM percent and free RAM minimum; default 4 concurrent jobs and 4 per-job threads with optional caps unset.

Parallelism is across distinct AFS volumes; one vos dump stream is indivisible. Bandwidth throttling and host admission are implemented cooperatively. Remote SSH host resource guarantees require additional approved remote instrumentation/cgroups. See [Kubernetes/cluster EN](CLUSTER_ORCHESTRATION_R039_EN.md) and [IT](CLUSTER_ORCHESTRATION_R039_IT.md).

## r039 — approved executable profiles and process-wide concurrency

Approving an agent device is not enough to authorize arbitrary host commands. On the agent host, create and review profiles/ID.json and run `multi-backup-agent preview --config PATH --profile ID` to obtain the exact displayed `profile_sha256` and typed vos plan. An authorized administrator separately calls **POST /api/agents/admin/profile** with admin session, CSRF and independent recovery authorization:

~~~json
{"agent_id":"afs-host-01","profile_id":"volume-test",
 "profile_sha256":"REPLACE_WITH_ACTUAL_64_HEX_DIGIT_DIGEST","provider":"afs"}
~~~

The server pins this exact agent/profile/hash/provider binding; pull enqueue and push/pull uploads fail closed on missing, changed or revoked profiles. A preflight rejects unauthorized uploads before accepting large ciphertext bodies, and the receipt transaction validates the pin again. Every profile edit requires new approval.

The local agent enforces max_concurrent_jobs across **independent CLI processes**, not only jobs in a single poll loop, using private disk-backed worker slots locked until the job/upload finishes. It rejects a new job when no slot is free. Do not clone a device identity or token across hosts.

