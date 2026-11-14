# Multi Backup r039 — Parallel backup execution and multi-replica server operations

**Updated 15 November 2026.** This is a development preview on PR #11, not a certified release candidate. Read [AFS agent operations](AGENTS_AFS_R039_EN.md) and [r038 operator manual](OPERATIONS_R038_EN.md) for the previous baseline.

## Common source-transport choices

All input plugins may declare one of four source preparation modes: server_local, ssh_agentless, managed_agent_pull, or managed_agent_push. Declaring a transport does NOT enable an unimplemented plugin adapter. The current Rust server worker executes only POSIX/NFS/SMB/SSHFS/GPFS CES filesystem source previews through the server_local transport. AFS has validated local and pinned-host SSH vos dump command generation plus a separately authenticated outbound agent path. The server never executes untrusted script text received from agents.

For AFS, a single vos dump is **one indivisible volume stream**; parallelism means independent volumes/backups running simultaneously, not splitting one volume into arbitrary segments. Full and incremental AFS dump chain timestamps must be preserved and independently verified.

## Parallelism and host-resource policy

Resource policy schema (ExecutionLimits):

~~~json
{
 "max_concurrent_jobs":4,
 "threads_per_job":4,
 "max_transfer_bytes_per_sec":20971520,
 "max_host_cpu_percent":75.0,
 "max_host_memory_percent":80.0,
 "min_free_memory_bytes":1073741824
}
~~~

Units are concurrent backup jobs, worker threads per job, **bytes/sec per stream**, host CPU usage percentage, host RAM used percentage and minimum free host RAM bytes. Defaults: 4 jobs and 4 threads; optional bandwidth and host-resource thresholds are disabled unless set. Valid jobs: 1–256, threads: 1–64, at most 2048 workers collectively. CPU/RAM percentages must be (0,100]; enabled transfer cap at least 1024 bytes/s.

For **pull**, the server supplies agent policy in the signed TLS session; agent and server limits are intersected to the stricter value. For **push**, an agent initiates work independently of server scheduling but first authenticates with the server and retrieves its current resource ceiling. A server-side upload handler also throttles incoming ciphertext transfer. AFS/SSH dump streams are bandwidth limited and have cooperative host CPU/RAM checks. **These are admission and cooperative controls, not a hard kernel cgroup/Job-Object enforcement guarantee.** The source host's real CPU/RAM load cannot be enforced over agentless SSH without a separately approved remote probe/cgroup wrapper. Intra-job threads_per_job does not accelerate an indivisible vos dump; file/chunk adapters require independent implementations before claiming that level of parallelism.

## PostgreSQL control plane, Kubernetes and storage placement

Each replica publishes the same HTTPS interface and queries PostgreSQL for the authoritative job catalog, storage inventory, approved agents and authenticated user sessions. PostgreSQL connections require certificate and hostname verification (sslmode=verify-full). Replicas emit heartbeats every 20 seconds; a replica becomes ineligible after 90 seconds without heartbeat. Jobs are claimed atomically with row-level SKIP LOCKED leases. A lease token controls heartbeat and outcome submission; after an interruption, replay/fencing of the underlying physical writes must still be tested.

A storage location has either:

- **shared**: an explicitly approved truly shared medium accessible to multiple registered replicas. Only independent jobs on this storage may be balanced across eligible replicas. A matching mount path alone is not evidence of shared ownership.
- **node_local**: storage physically accessible on one replica only, with an explicit owner_replica. The metadata is visible from every replica but the queue denies assignment to other replicas. Restoring files from an arbitrary pod requires separately authenticated internal proxy/routing, still pending.

An object cloud backend only supports distributed execution after a real output adapter and concurrency-safe commit implementation exists. Catalog-only cloud plugins are NOT operational by virtue of the cluster queue.

Example environment values (never commit real passwords or keys):

~~~text
MULTI_BACKUP_CLUSTER_DATABASE_URL=postgresql://user:REDACTED@postgres.example:5432/multi_backup?sslmode=verify-full
MULTI_BACKUP_REPLICA_ID=<unique Kubernetes Pod name>
MULTI_BACKUP_REPLICA_MAX_JOBS=4
MULTI_BACKUP_CLUSTER_STORAGE_IDS=shared-cephfs,local-node-one
MULTI_BACKUP_CLUSTER_EXECUTOR=enabled
MULTI_BACKUP_PUBLIC_ORIGIN=https://backups.example.org
MULTI_BACKUP_AGENT_UPLOAD_ROOT=/encrypted/shared-agent-upload
MULTI_BACKUP_AGENT_PROXY_SECRET_FILE=/run/secrets/agent-proxy
~~~

Use Kubernetes Secrets for credentials and independently verify that a shared mount supports the necessary locking, age encryption and atomic manifest semantics. The separate agent ingress interface is enabled only behind a trusted HTTPS proxy that overwrites **both** x-forwarded-proto=https and x-mb-trusted-https-proxy with a private shared secret. The client must not be able to supply or override these fields. If the secret/TLS/cluster database is unavailable, agent routes fail closed. Never expose the internal HTTP pod port publicly.

### Administrative endpoints

All administrative POSTs require authenticated admin, same-origin CSRF and the existing private gateway. Device enrollment/approval/revocation also requires independent recovery authorization.

| Route | Purpose |
| --- | --- |
| GET /api/cluster/status | Shared replica/storage/job status |
| POST /api/cluster/storage | Register shared or owner-bound node_local storage; shared requires confirmed_shared |
| POST /api/cluster/jobs | Queue work with source plugin, transport, storage and ExecutionLimits |
| GET /api/agents/admin/list | Inventory approved/revoked devices |
| POST /api/agents/admin/enroll | Register device ID/user/public key, return token once |
| POST /api/agents/admin/approve | Explicit device approval |
| POST /api/agents/admin/revoke | Revoke device and invalidate its further requests |
| POST /api/agents/admin/limits | Set per-agent server policy |
| POST /api/agents/admin/enqueue | Queue a signed-profile managed-agent pull job |

Agent requests GET /api/agents/policy, GET /api/agents/poll and POST /api/agents/upload additionally require registered Ed25519 signatures, user identity, bearer token, TLS proxy attestation, SHA256 body and metadata checks, timestamp and PostgreSQL-persisted unique nonces. Concurrent out-of-order requests are supported within a bounded replay window.

Example shared storage registration:

~~~json
{"storage":{"storage_id":"shared-cephfs","backend":"posix",
 "visibility":"shared","owner_replica":null},"confirmed_shared":true}
~~~

Example filesystem work item (POST /api/cluster/jobs):

~~~json
{"job_id":"docs-001","plugin_id":"posix","storage_id":"shared-cephfs",
 "source_transport":"server_local","priority":0,
 "execution_limits":{"max_concurrent_jobs":4,"threads_per_job":4,
   "max_transfer_bytes_per_sec":20971520},
 "job_spec":{"source":"/data/source","store":"/data/encrypted-store",
  "identity":"/run/secrets/age-identity","label":"documents",
  "namespace":"files","mode":"full","snapshot_attested":false}}
~~~

The server's cluster filesystem executor must be explicitly enabled; SSH/agent transport jobs are not silently executed as shell commands by Kubernetes pods. The agent pull queue remains a separate authorized dispatch channel. A completed upload is an authenticated receipt of encrypted data, not yet an independent successful decrypt-and-AFS-restore certification.

### User interface across replicas

When clustering is enabled, browser sessions and logout are recorded in PostgreSQL, so the same cookie can be recognized by another replica. All replicas can expose common cluster/status metadata. Existing pre-login OIDC state is still held in a replica-local map; stickiness or migration of those pending challenges is required for cross-pod OIDC callbacks. A node-local source cannot be read by a different pod merely because its metadata is listed.

## Qualification checklist

Test with at least two Kubernetes replicas, one truly shared store and one node_local store: local jobs never run on non-owner nodes; shared independent jobs distribute; tasks cannot be double-claimed; simulate expired and lost leases, writers still holding data, server failovers and replays. Test approved/pending/revoked agent with concurrent push and pull, bandwidth, host memory/CPU caps, TLS proxy spoofing and real AFS dumps/restores. Verify installed PKG/EXE/DEB/RPM on every declared architecture, update/rollback, certificate renewal, server-wide bandwidth/CPU accounting and distributed garbage collection without data loss.

**Not yet RC-qualified:** exactly-once write fencing after lease loss, global aggregate bandwidth ceilings across replicas, remote host hard CPU/RAM enforcement over SSH, true generic cloud output adapters, fully common cluster browse/restore routing, OIDC pending state sharing, full agent enrollment automation, actual AFS incremental restore on a new server, signed installers and independent security review. These cannot be described as completed by a passing compile alone.

See [Italian companion](CLUSTER_ORCHESTRATION_R039_IT.md), [agent manual EN](AGENTS_AFS_R039_EN.md), [feature inventory](FEATURE_INVENTORY_R038.md), [RC gates](RC_CERTIFICATION_MATRIX.md), [handoff](HANDOFF.md).

## r039 unified web cluster and agent overview

The Rust server GUI includes a read-only administrator **Cluster, agenti e backup paralleli** section, built from GET /api/cluster/status and GET /api/agents/admin/list. Each replica displays the same PostgreSQL inventory of live servers, capabilities, registered shared versus node-local stores, queued/running/finished jobs and agents awaiting approval, online or revoked. The dashboard never interprets database-sourced names as HTML. It does not yet implement the complete administration editor: enrolled agent approval/limits and storage registration remain protected API actions. Verify browser sessions and cluster state with a real multi-pod load balancer before claiming unified HA production readiness.

