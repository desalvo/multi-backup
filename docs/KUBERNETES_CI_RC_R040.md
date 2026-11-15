# Multi Backup r040 — genuine Kubernetes acceptance in GitHub CI

**Status:** development integration, not a released RC. Source branch: `feature/r040-real-kubernetes-ci`; base: PR #11 `feature/r039-afs-agent-packages`.

## What the CI genuinely provisions

After the existing Production Gate and Rust server image validation, the **kubernetes-real-cluster** job uses upstream **kind** to boot a Kubernetes control plane plus **two independently scheduled worker nodes**, including real kubelet, API server, etcd, scheduler, CNI and Services on a GitHub-hosted Linux Docker runner. It builds the exact local Rust OCI image from the PR, loads it into kind and runs **two live Rust server pods** with mandatory different-node pod anti-affinity. A genuine PostgreSQL 16 Pod uses an ephemeral, self-signed CA and a server certificate with correct Kubernetes Service DNS SANs. Both Rust server pods connect with `sslmode=verify-full`, a separately trusted CA, and independently register their real pod IDs in the shared SQL catalog.

The test checks readiness, anti-affinity, live SQL registrations, networked ClusterIP access via port-forward, required private-gateway authentication, rejection of unauthenticated cluster admin APIs, and graceful reconciliation of pod replacement without losing the functioning Service or SQL registration.

Test credentials are random, generated only inside the job and never committed. A test-only public Argon2id hash is the same immutable synthetic fixture as the existing OCI gateway smoke. Kubernetes Secrets are copied in an init container into an in-memory `emptyDir` owned by unprivileged UID 10001 with `0600` permissions. The Rust server runs without privilege escalation and with a read-only root filesystem. Test namespaces and VMs are ephemeral.

## What this **does not** certify

Kind workers are separate real Kubernetes nodes but they run as Docker containers **on one physical CI VM**, not as three independent VMs or production failure domains. PostgreSQL is a genuine running instance, **not HA**. Each server pod has its own temporary `emptyDir`: there is deliberately no claim of shared RWX backup-store correctness, durable persistence or safely balanced filesystem writer jobs. `MULTI_BACKUP_CLUSTER_EXECUTOR=disabled` is compulsory until distributed encrypted-write physical fencing and crash/replay tests pass.

Before RC approval, require a separate external multi-host Kubernetes acceptance with a proven RWX store (NFS/CephFS), node-local owner-only scheduling, shared work distribution, lease-expiry/quarantine/physical writer termination, interrupted uploads, ciphertext consistency, network partitions and fail-closed recovery. Confirm restore of real backed-up data, multi-node PostgreSQL HA behavior, and package/signing/security tests. Do not use this CI green result to publish a stable version by itself.

## Operations and failure diagnostics

Run the test in `.github/workflows/ci.yml`, job `kubernetes-real-cluster`; manual CI dispatch also exercises it. Test script: `tests/kubernetes/test-kind-cluster.sh`. Declarative resources: `tests/kubernetes/ci-cluster.yaml`; kind topology: `tests/kubernetes/kind-config.yaml`. On failures the workflow collects pod logs, events, workload status and rollout diagnoses; the cluster is always deleted. It blocks both container and native release publication by being a required `needs` job.

For an independently managed multi-host runner, keep the same namespace, database TLS, pod spread, authenticated API and failover assertions; replace the disposable `kind` bootstrap with access to the externally managed cluster and explicitly test its RWX implementation. Never deploy these synthetic credentials to a real environment.

This deliverable adds a real Kubernetes CI gate, **not** the general-purpose Kubernetes source backup plugin, a fully HA file store or RC certification.
