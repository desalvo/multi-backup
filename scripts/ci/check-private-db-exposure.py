#!/usr/bin/env python3
"""Fail-closed deployment contract: database and consensus ports stay private.

Offline static preflight only; it is NOT a live firewall or penetration test.
"""
from pathlib import Path
import ipaddress
import re
import sys
import yaml

ROOT=Path(__file__).resolve().parents[2]
PORTS={5432,2379,2380,8008}

def check_kubernetes(resources):
    if not isinstance(resources,list) or not resources:
        raise ValueError("Kubernetes deployment manifest missing")
    for obj in resources:
        if not isinstance(obj,dict):
            continue
        kind=obj.get("kind")
        if kind=="Service":
            spec=obj.get("spec") or {}
            if spec.get("type","ClusterIP") in ("NodePort","LoadBalancer"):
                raise ValueError("Backup service may not expose a public Service")
            for p in spec.get("ports") or []:
                for k in ("port","targetPort","nodePort"):
                    v=p.get(k)
                    if isinstance(v,int) and v in PORTS:
                        raise ValueError("PostgreSQL, etcd or Patroni port exposed by Service")
        if kind in ("Deployment","StatefulSet","DaemonSet"):
            for container in (((obj.get("spec") or {}).get("template") or {})
                     .get("spec") or {}).get("containers") or []:
                for p in container.get("ports") or []:
                    if p.get("containerPort") in PORTS or p.get("hostPort") in PORTS:
                        raise ValueError("Database cluster control port exposed on a pod")
            spec=((obj.get("spec") or {}).get("template") or {}).get("spec") or {}
            if spec.get("hostNetwork") is True:
                raise ValueError("Application pod must not use hostNetwork")
    return True

def check_private_config(haproxy,patroni,etcd,server_env):
    if "bind 127.0.0.1:6432" not in haproxy:
        raise ValueError("PostgreSQL HAProxy must bind only loopback")
    for line in haproxy.splitlines():
        if line.strip().startswith("server "):
            parts=line.strip().split()
            if len(parts)<3 or not parts[2].endswith(":5432"):
                raise ValueError("Unexpected HAProxy database backend")
            ip=ipaddress.ip_address(parts[2].rsplit(":",1)[0])
            if not ip.is_private or ip.is_loopback:
                raise ValueError("Database backend must be on private VLAN")
            if "verify required" not in line or "ca-file" not in line:
                raise ValueError("Patroni TLS checks must be verified")
    data=yaml.safe_load(patroni)
    if not isinstance(data,dict):raise ValueError("Invalid Patroni YAML")
    rest=data["restapi"]["listen"]
    pg=data["postgresql"]["listen"]
    for name,addr,port in [("Patroni",rest,8008),("PostgreSQL",pg,5432)]:
        ip,sep,rawport=addr.rpartition(":")
        if not sep or int(rawport)!=port:
            raise ValueError(f"Unexpected {name} listener port")
        address=ipaddress.ip_address(ip)
        if not address.is_private or address.is_loopback:
            raise ValueError(f"{name} must listen only on private VLAN")
    for line in etcd.splitlines():
        if line.startswith(("ETCD_LISTEN_CLIENT_URLS=","ETCD_LISTEN_PEER_URLS=",
                            "ETCD_ADVERTISE_CLIENT_URLS=","ETCD_INITIAL_ADVERTISE_PEER_URLS=")):
            uri=line.split("=",1)[1]
            if not uri.startswith("https://10.70.0."):
                raise ValueError("etcd private endpoint/TLS requirement bypassed")
    if not re.search(r'^MULTI_BACKUP_CLUSTER_DATABASE_URL=postgresql://[^\n]+@db-ha\.internal:6432/[^\n]+\?sslmode=verify-full$',server_env,re.M):
        raise ValueError("Application DB must connect via validated private primary router TLS")
    if "MULTI_BACKUP_REST_API_ENABLED=false" not in server_env or (
        "MULTI_BACKUP_PROMETHEUS_ENABLED=false" not in server_env):
        raise ValueError("Monitoring must default disabled")
    return True

def main():
    path=ROOT/"k8s/multi-backup-cluster-r039.preview.yaml"
    resources=list(yaml.safe_load_all(path.read_text()))
    check_kubernetes(resources)
    check_private_config(
        (ROOT/"packaging/ha/haproxy-postgres.cfg.example").read_text(),
        (ROOT/"packaging/ha/patroni-node.yaml.example").read_text(),
        (ROOT/"packaging/ha/etcd-node.env.example").read_text(),
        (ROOT/"packaging/linux/server.env.example").read_text())
    print("PASS: no external DB/DCS/Patroni port in declared manifests and private HA endpoints")


if __name__=="__main__":
    try:main()
    except (ValueError,KeyError,OSError,TypeError) as exc:
        print(f"FAIL: private DB isolation: {exc}",file=sys.stderr)
        raise SystemExit(1) from None
