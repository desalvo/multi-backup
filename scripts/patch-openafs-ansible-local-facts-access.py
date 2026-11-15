#!/usr/bin/env python3
"""Keep a real OpenAFS Ansible lab reusable with secure non-root facts access.

The original openafs-contrib.local_install playbook creates volumes as the
unprivileged runner. Its earlier facts writers run with become, and inherit
the runner's restrictive umask 077, so openafs.fact may be mode 0600/root.
The volume module catches EACCES and reports misleading "Unable to locate
viceetcdir directory". After installing the AFS client (checkpoint 3),
provide group-only traverse access to the parent directories and group-only
read access to real facts, then assert that the unprivileged
runner can read the real cacheinfo path before volume creation.

Only applies to the SHA-pinned upstream local_install.yaml in isolated CI.
No credentials, keytabs or secrets are changed or made world-readable.
"""
import argparse
from pathlib import Path

MARKER = "    - name: Checkpoint 4\n"
FIX = """    - name: Allow group traverse only on exact Ansible facts directories
      become: true
      ansible.builtin.file:
        path: "{{ item }}"
        state: directory
        owner: root
        group: "{{ afs_user }}"
        mode: "0710"
      loop:
        - /etc/ansible
        - /etc/ansible/facts.d

    - name: Grant runner-group read-only access to non-secret OpenAFS installation facts
      become: true
      ansible.builtin.file:
        path: /etc/ansible/facts.d/openafs.fact
        owner: root
        group: "{{ afs_user }}"
        mode: "0640"

    - name: Grant runner-group read-only access to real AFS client cacheinfo
      become: true
      ansible.builtin.file:
        path: /etc/openafs/cacheinfo
        owner: root
        group: "{{ afs_user }}"
        mode: "0640"

    - name: Prove the non-root AFS volume module can read authentic client path facts
      ansible.builtin.command:
        argv:
          - python3
          - -c
          - "import json,os; f=json.load(open('/etc/ansible/facts.d/openafs.fact')); d=f.get('dirs',{}).get('viceetcdir'); assert d and os.path.isfile(os.path.join(d,'cacheinfo')), 'Real AFS cacheinfo missing'; open(os.path.join(d,'cacheinfo')).read()"
      changed_when: false

"""


def patch(path: Path) -> str:
    source = path.read_text(encoding="utf-8")
    if source.count(FIX) == 1:
        return "already patched"
    if source.count(MARKER) != 1:
        raise ValueError("Unexpected pinned upstream playbook layout; refusing modification")
    if "    - name: Set checkpoint 3" not in source or "openafs_volume:" not in source:
        raise ValueError("Required client / volume tasks missing; refusing modification")
    output = source.replace(MARKER, FIX + MARKER, 1)
    path.write_text(output, encoding="utf-8")
    return "group-only directory traverse, file read and non-root preflight added"


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("playbook", type=Path)
    args = p.parse_args()
    if args.playbook.name != "local_install.yaml":
        p.error("Only SHA-pinned upstream local_install.yaml is supported")
    print("PASS: " + patch(args.playbook))
