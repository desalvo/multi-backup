#!/usr/bin/env python3
"""Real PG16/Patroni + 3 TLS etcd network partition: separate Linux netns.
Runs only as an unprivileged GitHub-hosted CI user with bounded sudo.
One ephemeral host, three independent network stacks; NOT three physical hosts.
"""
import importlib.util
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import time

SOURCE = Path(__file__).with_name("patroni-etcd-real-ha.py")
IPS = ["10.239.143.11", "10.239.143.12", "10.239.143.13"]

def sudo(*args, check=True):
    return subprocess.run(["sudo", "-n", *args], capture_output=True,
                          text=True, timeout=20, check=check)

class Lab:
    def __init__(self):
        prefix = f"mbh{os.getpid()%100000}"
        self.bridge = prefix+"br"
        self.names = [prefix+f"n{i}" for i in range(3)]
        self.peers = [prefix+f"h{i}" for i in range(3)]
        self.created = []
        self.rules = []
        self.host_bridge_rule = False
        self.children = []
        self.uid, self.gid = os.getuid(), os.getgid()

    def create(self):
        sudo("ip","link","add",self.bridge,"type","bridge")
        self.created.append(("bridge",self.bridge))
        sudo("ip","addr","add","10.239.143.1/24","dev",self.bridge)
        sudo("ip","link","set",self.bridge,"up")
        for i,ns in enumerate(self.names):
            sudo("ip","netns","add",ns)
            self.created.append(("ns",ns))
            sudo("ip","link","add",self.peers[i],"type","veth",
                 "peer","name","mb-eth")
            sudo("ip","link","set","mb-eth","netns",ns)
            sudo("ip","link","set",self.peers[i],"master",self.bridge)
            sudo("ip","link","set",self.peers[i],"up")
            sudo("ip","-n",ns,"addr","add",IPS[i]+"/24","dev","mb-eth")
            sudo("ip","-n",ns,"link","set","lo","up")
            sudo("ip","-n",ns,"link","set","mb-eth","up")
            result=sudo("ip","-n",ns,"addr","show","mb-eth").stdout
            if IPS[i] not in result: raise RuntimeError("Namespace interface not configured")
        # GitHub Ubuntu runners may apply a DROP policy to traffic
        # bridged between veth ports. Permit ONLY the ephemeral /24 across
        # our isolated bridge; restore this exact host firewall rule on exit.
        sudo("iptables","-I","FORWARD","-i",self.bridge,"-o",self.bridge,
             "-s","10.239.143.0/24","-d","10.239.143.0/24","-j","ACCEPT")
        self.host_bridge_rule=True
        for i,ns in enumerate(self.names):
            for j,peer in enumerate(IPS):
                if i==j: continue
                probe=sudo("ip","netns","exec",ns,
                           "ping","-c","1","-W","2",peer,check=False)
                if probe.returncode:
                    diagnostics={
                        "probe_stdout":probe.stdout[-400:],
                        "probe_stderr":probe.stderr[-400:],
                        "source_routes":sudo("ip","-n",ns,"route",check=False).stdout[-400:],
                        "source_arp":sudo("ip","-n",ns,"neigh",check=False).stdout[-400:],
                        "dest_link":sudo("ip","-n",self.names[j],"addr","show",
                                         check=False).stdout[-400:],
                        "bridge":sudo("ip","-d","link","show",self.bridge,
                                      check=False).stdout[-400:],
                        "host_forward":sudo("iptables","-S","FORWARD",
                                            check=False).stdout[-700:]
                    }
                    raise RuntimeError(f"Preflight veth {i+1}->{j+1} ICMP fails: {diagnostics}")
        print("PROOF: all six directed namespace-to-namespace ICMP probes passed",
              flush=True)
        print("PROOF: three real network namespaces with separate veth stacks",flush=True)

    def start(self, logfile, *command):
        if command[0]=="etcd":
            names=[re.fullmatch(r"--name=mb-etcd([123])",c)
                   for c in command[1:] if c.startswith("--name=")]
            if len(names)!=1 or names[0] is None: raise RuntimeError("Wrong etcd identity")
            i=int(names[0].group(1))-1
        elif command[0]=="patroni":
            name=re.fullmatch(r"patroni-([123])\.yaml",Path(command[1]).name)
            if name is None: raise RuntimeError("Wrong Patroni identity")
            i=int(name.group(1))-1
        else: raise RuntimeError("Unauthorized netns executable")
        argv=["sudo","-n","ip","netns","exec",self.names[i],
              "setpriv","--reuid",str(self.uid),"--regid",str(self.gid),
              "--clear-groups","--",*command]
        with logfile.open("a") as log:
            p=subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=log,
                               stderr=subprocess.STDOUT,start_new_session=True)
        self.children.append(p)
        return p

    def rule(self,i,chain,peer,add):
        if peer not in IPS or chain not in ("INPUT","OUTPUT"):
            raise RuntimeError("Refusing unrelated firewall mutation")
        selector="-s" if chain=="INPUT" else "-d"
        sudo("ip","netns","exec",self.names[i],"iptables",
             "-I" if add else "-D",chain,selector,peer,"-j","DROP")
        if add: self.rules.append((i,chain,peer))

    def partition(self,i):
        for peer in (p for k,p in enumerate(IPS) if k!=i):
            self.rule(i,"INPUT",peer,True)
            self.rule(i,"OUTPUT",peer,True)
        rules=sudo("ip","netns","exec",self.names[i],"iptables","-S").stdout
        for peer in (p for k,p in enumerate(IPS) if k!=i):
            if f"-s {peer}/32 -j DROP" not in rules or f"-d {peer}/32 -j DROP" not in rules:
                raise RuntimeError("Firewall partition not effective")
        print(f"PROOF: real iptables DROP in isolated old primary {i}",flush=True)

    def reconnect(self):
        while self.rules:
            i,chain,peer=self.rules[-1]
            self.rule(i,chain,peer,False)
            self.rules.pop()

    def cleanup(self):
        try: self.reconnect()
        except Exception as e:
            print("Disposable rule cleanup warning:",type(e).__name__,flush=True)
        if self.host_bridge_rule:
            sudo("iptables","-D","FORWARD","-i",self.bridge,"-o",self.bridge,
                 "-s","10.239.143.0/24","-d","10.239.143.0/24","-j","ACCEPT",
                 check=False)
            self.host_bridge_rule=False
        for p in self.children:
            if p.poll() is None:
                try: os.killpg(p.pid,signal.SIGTERM)
                except ProcessLookupError: pass
        time.sleep(2)
        for kind,name in reversed(self.created):
            if kind=="ns":
                pids=sudo("ip","netns","pids",name,check=False).stdout.split()
                for pid in pids:
                    if pid.isdecimal(): sudo("kill","-TERM",pid,check=False)
                sudo("ip","netns","del",name,check=False)
            else: sudo("ip","link","del",name,check=False)
        print("CLEANUP: isolated runner network removed",flush=True)

def main():
    if (os.environ.get("GITHUB_ACTIONS")!="true" or
        os.environ.get("RUNNER_ENVIRONMENT")!="github-hosted" or os.geteuid()==0):
        raise SystemExit("Refused outside disposable unprivileged GitHub-hosted CI")
    for cmd in ("sudo","ip","iptables","setpriv"):
        if not shutil.which(cmd): raise RuntimeError(f"Missing {cmd}")
    if sudo("true",check=False).returncode: raise RuntimeError("CI cannot use bounded sudo")
    spec=importlib.util.spec_from_file_location("real_patroni_ci",SOURCE)
    ha=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ha)
    lab=Lab()
    os.environ["MULTI_BACKUP_HA_NETWORK_ISOLATION_REQUIRED"]="true"
    try:
        lab.create()
        ha.ADDRESSES=IPS
        original=ha.conf
        def private_conf(root,index):
            filename,data=original(root,index)
            contents=filename.read_text()
            if "127.0.0.0/8" not in contents: raise RuntimeError("Bad PG HBA fixture")
            filename.write_text(contents.replace("127.0.0.0/8","10.239.143.0/24"))
            filename.chmod(0o600)
            return filename,data
        ha.conf=private_conf
        ha.start=lab.start

        def network_fault_drill(root,initial,procs):
            peer=ha.wait_for("DCS synchronous peer",lambda:
                ha.dcs_sync_candidate(root,initial),procs,100)
            if peer==initial: raise RuntimeError("Synchronous peer not independent")
            ha.pg(root,initial,
                  "INSERT INTO ha_witness VALUES (4,'acknowledged-before-network-split')")
            survivors=tuple(i for i in range(3) if i!=initial)
            try:
                lab.partition(initial)
                next_leader=ha.wait_primary(root,survivors,procs,130)
                if next_leader==initial: raise RuntimeError("No independent election")
                ha.wait_for("old primary fenced",lambda:
                    not ha.role(root,initial,"primary"),procs,75)
                data=ha.pg(root,next_leader,
                  "SELECT payload FROM ha_witness WHERE id=4").stdout.strip()
                if data!="acknowledged-before-network-split":
                    raise RuntimeError("Synchronous WAL lost after network partition")
                try:
                    stale=ha.pg(root,initial,
                      "INSERT INTO ha_witness VALUES (5,'stale-isolated-primary')",
                      expect_ok=False,query_timeout_ms=2000)
                    if stale.returncode==0:
                        raise RuntimeError("UNSAFE: stale primary ACKed write without DCS")
                except subprocess.TimeoutExpired:
                    print("PROOF: isolated primary did not ACK before deadline",flush=True)
                print("PASS: actual netns split, preserved ACK and fenced stale primary",flush=True)
            finally:
                lab.reconnect()
            ha.wait_for("old isolated leader rejoined as replica",
                lambda:ha.role(root,initial,"replica"),procs,140)
            if len(ha.primaries(root))!=1:
                raise RuntimeError("UNSAFE: duplicate primaries after partition heal")
            print("PASS: network heal, one primary, old leader rejoined",flush=True)
        ha.network_fault_drill=network_fault_drill
        ha.main()
        print("PASS: REAL NETWORK NAMESPACE PG16 Patroni/etcd mTLS fault injection",
              flush=True)
    finally:
        lab.cleanup()

if __name__=="__main__":
    main()
