#!/usr/bin/env python3
"""Guarded, create-only nested oVirt/ESXi KVM lab VM provisioner.

This prepares guests, NOT a configured oVirt Engine or licensed vCenter.
No download of proprietary media, no deletion/overwrite, no host network edits.
Only an explicitly isolated pre-existing libvirt network can be used.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import xml.etree.ElementTree as ET

PREFIX = "mb-r213-"
ALLOWED_ROOT = Path("/var/lib/multi-backup-ci/r213")
SENTINEL = Path("/etc/multi-backup-ci/hypervisor-lab-r213.allow")
APPROVAL = "YES_CREATE_ONLY_DISPOSABLE_OVIRT_VMWARE_LAB"
SAFE_NAME = re.compile(r"^mb-r213-[a-z][a-z0-9-]{1,40}$")
HASH = re.compile(r"^[0-9a-f]{64}$")

def fail(message):
    raise SystemExit(f"DENIED: {message}")

def file_safe(p, *, owner_only=False):
    file = Path(p)
    if not file.is_absolute() or file.is_symlink():
        fail("lab media and config must be absolute non-symlink files")
    st = file.stat()
    if not stat.S_ISREG(st.st_mode):
        fail("lab media must be regular files")
    if owner_only and st.st_mode & 0o077:
        fail("cloud-init user-data must be owner-only 0600")
    return file

def digest(p):
    sha = hashlib.sha256()
    with open(p, "rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()

def read(args, *, full=False):
    cfg_path = file_safe(args.config)
    cfg = json.loads(cfg_path.read_text())
    expected = {"network", "engine_image", "host_image", "engine_user_data",
                "host_user_data", "esxi_iso", "sha256", "enable_esxi"}
    if set(cfg) != expected:
        fail("unexpected/missing lab configuration keys")
    net = cfg["network"]
    if not SAFE_NAME.fullmatch(net):
        fail("libvirt network name must start with mb-r213-")
    for field in ("engine_image", "host_image", "engine_user_data", "host_user_data"):
        if not isinstance(cfg[field], str):
            fail("lab paths must be strings")
    if not isinstance(cfg["enable_esxi"], bool):
        fail("enable_esxi must be boolean")
    for p in ("engine_image", "host_image"):
        if not str(cfg[p]).endswith(".qcow2"):
            fail("oVirt nested guest source disk must be qcow2")
    hashes = cfg["sha256"]
    if not isinstance(hashes, dict) or set(hashes) != (
        {"engine_image", "host_image", "esxi_iso"} if cfg["enable_esxi"]
        else {"engine_image", "host_image"}
    ):
        fail("exact input media SHA-256 keys are required")
    paths = ["engine_image", "host_image", "engine_user_data", "host_user_data"]
    if cfg["enable_esxi"]:
        paths.append("esxi_iso")
    for key in paths:
        value = cfg[key]
        if not isinstance(value, str) or not value.startswith("/"):
            fail("all lab media and cloud-init paths must be absolute")
        if full:
            file_safe(value, owner_only=key.endswith("_user_data"))
    for key in hashes:
        if not isinstance(hashes[key], str) or not HASH.fullmatch(hashes[key]):
            fail("every supplied media digest must be lowercase SHA256")
        if full and digest(cfg[key]) != hashes[key]:
            fail("a lab media file mismatched its explicitly pinned SHA256")
    return cfg

def commands(cfg):
    names = [PREFIX + "ovirt-engine", PREFIX + "ovirt-host"]
    commands = []
    specs = [
        ("ovirt-engine", cfg["engine_image"], cfg["engine_user_data"], 8192, 4),
        ("ovirt-host", cfg["host_image"], cfg["host_user_data"], 16384, 8),
    ]
    for short, source, cloud, mem, cpus in specs:
        output = ALLOWED_ROOT / (PREFIX + short + ".qcow2")
        name = PREFIX + short
        commands.append([
            ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2",
             "-b", source, str(output)],
            ["virt-install", "--connect", "qemu:///system", "--name", name,
             "--memory", str(mem), "--vcpus", str(cpus),
             "--cpu", "host-passthrough", "--import",
             "--disk", f"path={output},format=qcow2,bus=virtio",
             "--network", f"network={cfg['network']},model=virtio",
             "--os-variant", "almalinux9",
             "--cloud-init", f"user-data={cloud}",
             "--graphics", "none", "--noautoconsole"],
        ])
    if cfg["enable_esxi"]:
        name = PREFIX + "esxi8"
        output = ALLOWED_ROOT / (name + ".qcow2")
        commands.append([
            ["qemu-img", "create", "-f", "qcow2", str(output), "100G"],
            ["virt-install", "--connect", "qemu:///system", "--name", name,
             "--memory", "16384", "--vcpus", "8", "--cpu", "host-passthrough",
             "--machine", "q35",
             "--disk", f"path={output},format=qcow2,bus=sata",
             "--network", f"network={cfg['network']},model=e1000e",
             "--os-variant", "generic", "--cdrom", cfg["esxi_iso"],
             "--graphics", "vnc,listen=127.0.0.1", "--noautoconsole"],
        ])
        names.append(name)
    return names, commands

def real_environment(cfg, names):
    if (os.environ.get("RUNNER_ENVIRONMENT") != "self-hosted" or
        os.environ.get("MB_LAB_PROVISION") != APPROVAL or os.geteuid() == 0):
        fail("only an approved non-root self-hosted runner may provision")
    p = file_safe(SENTINEL)
    proof = p.stat()
    if proof.st_uid != 0 or proof.st_mode & 0o077:
        fail("root-owned 0600 physical lab sentinel required")
    if p.read_text().strip() != APPROVAL:
        fail("physical lab sentinel not authorized")
    if not Path("/dev/kvm").exists():
        fail("nested KVM device unavailable; enable BIOS VT-x/AMD-V and host nested virtualization")
    try:
        cpu = Path("/proc/cpuinfo").read_text()
        if not any(flag in cpu for flag in (" vmx", " svm")):
            fail("host CPU virtualization extension not exposed")
        root = ALLOWED_ROOT
        if root.is_symlink() or not root.is_dir() or root.resolve() != root:
            fail("lab output directory missing, symlinked or non-canonical")
        if root.stat().st_uid != os.geteuid() or root.stat().st_mode & 0o077:
            fail("lab output directory must belong to runner and be mode 0700")
        net = subprocess.run(["virsh", "-c", "qemu:///system", "net-dumpxml",
                              cfg["network"]], capture_output=True, text=True, check=True)
        xml = ET.fromstring(net.stdout)
        if xml.tag != "network" or xml.findtext("name") != cfg["network"] or \
           xml.find("forward") is not None:
            fail("lab network must be a pre-created isolated libvirt network with no forwarding")
        for name in names:
            dom = subprocess.run(["virsh", "-c", "qemu:///system", "dominfo", name],
                                 capture_output=True)
            if dom.returncode == 0:
                fail("refusing any existing VM with the planned lab name")
            if (root / (name + ".qcow2")).exists():
                fail("refusing to reuse or overwrite existing guest disk")
    except (OSError, subprocess.CalledProcessError, ET.ParseError) as exc:
        fail(f"could not prove nested KVM and isolated virtual network: {type(exc).__name__}")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["plan", "provision"])
    parser.add_argument("--config", required=True)
    parser.add_argument("--confirm", default="")
    args = parser.parse_args()
    if args.action == "provision":
        if args.confirm != APPROVAL:
            fail("explicit create-only confirmation required")
        if (os.environ.get("RUNNER_ENVIRONMENT") != "self-hosted" or
            os.environ.get("MB_LAB_PROVISION") != APPROVAL or os.geteuid() == 0):
            fail("lab provisioning is forbidden outside approved non-root self-hosted runner")
    cfg = read(args, full=args.action == "provision")
    names, steps = commands(cfg)
    if args.action == "plan":
        # Never include secrets; cloud-init file paths only.
        print(json.dumps({"mode": "plan-only", "isolated_network": cfg["network"],
                          "virtual_machines": names, "commands": steps,
                          "actually_provisioned": False,
                          "engine_setup_complete": False,
                          "vcenter_licensed": False}, indent=2))
        return
    if args.confirm != APPROVAL:
        fail("explicit create-only confirmation required")
    real_environment(cfg, names)
    for commands_for_vm in steps:
        for command in commands_for_vm:
            subprocess.run(command, check=True, stdin=subprocess.DEVNULL)
    print(json.dumps({"mode": "provision", "created_vm": names,
                      "engine_setup_complete": False,
                      "esxi_install_complete": False,
                      "vcenter_licensed": False}, indent=2))

if __name__ == "__main__":
    main()
