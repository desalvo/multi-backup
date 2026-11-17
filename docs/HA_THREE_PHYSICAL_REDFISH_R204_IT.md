# Three **independent bare-metal** Patroni/etcd hosts — disruptive approval gate

This manual laboratory is **not** the 3-node Docker/Kubernetes/netns CI. It is reserved for dedicated, disposable physical hosts with separate power controllers, individual DMI and boot identities, strict known_hosts, independently pinned HTTPS Redfish BMCs, synchronous PostgreSQL and a mutually consistent Patroni cluster. Use it only when power interruption and cleanup are authorized by the operator.

## Runner and access

1. Connect an independent **non-root** Linux self-hosted Actions runner with exact labels `self-hosted, linux, multi-backup-ha-field`, outside the three target hosts. Protect environment `multi-backup-physical-destructive-lab` with an authorized approver.
2. Set a root-owned, group-readable, non-world-readable `/etc/multi-backup/physical-ha-lab.json` (mode `0640`, owner root, group dedicated CI runner), with no credentials in it:
```json
{
  "ca_bundle": "/etc/multi-backup/redfish-lab-ca.pem",
  "hosts": {
    "lab1.example.invalid": {
      "url": "https://bmc1.example.invalid",
      "system": "/redfish/v1/Systems/1",
      "product_uuid": "00000000-0000-0000-0000-000000000001"
    },
    "lab2.example.invalid": {
      "url": "https://bmc2.example.invalid",
      "system": "/redfish/v1/Systems/1",
      "product_uuid": "00000000-0000-0000-0000-000000000002"
    },
    "lab3.example.invalid": {
      "url": "https://bmc3.example.invalid",
      "system": "/redfish/v1/Systems/1",
      "product_uuid": "00000000-0000-0000-0000-000000000003"
    }
  }
}
```
The placeholders are **not real identities**. Replace with the operator-verified hardware UUIDs, dedicated TLS CA, endpoints and correct Redfish system URIs. Hostnames and UUIDs MUST correspond to different physical chassis with separate power domains.

3. Set environment secrets `MB_BMC1_USER/PASSWORD`, `MB_BMC2_USER/PASSWORD`, `MB_BMC3_USER/PASSWORD` (dedicated scoped BMC accounts; never production BMCs); install individually pinned SSH host keys; allow read-only Patroni and DMI inspection and dedicated `sudo -n -u postgres psql` on the lab hosts.
4. Use GitHub Actions **workflow_dispatch** only for `.github/workflows/ha-three-physical-redfish-r204.yml`, supply exact three approved node hostnames and type `YES_POWER_CYCLE_ONE_ISOLATED_TEST_HOST`. The protected environment must be approved separately.

## Mandatory acceptance

The workflow refuses hosted runners, root orchestration, unexpected hosts, duplicates of machine/boot/product/chassis identities, virtual hosts, untrusted/redirection-prone BMC endpoints, incorrectly mapped DMI UUIDs, unhealthy Patroni quorum or absent synchronous replication.

With successful preflight, it inserts a unique acknowledged witness row, issues **Redfish ForceOff only to the current lab primary**, observes the hardware power-off, requires election of one of the two surviving physical hosts and preservation of acknowledged PostgreSQL data, and then powers the original host back on in a `finally` block. It checks new kernel boot ID and re-convergence. A failed job must be investigated in person; do **not** assume automated recovery if the BMC itself is unreachable.

## Non-certifications

Even a green run proves only these three approved lab servers. It does not establish production BMC privilege isolation, switch/rack/cooling failures, remote storage shared-LUN fencing, operator/root sabotage resistance, nor third-party hypervisor guest consistency. **Until a protected manual workflow run has actually passed, physical three-host HA is NOT CERTIFIED.**

One-line English commit summary: Gate physical three-host synchronous PostgreSQL failover behind manually approved bare-metal Redfish power-cycle acceptance.
