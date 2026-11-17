#!/usr/bin/env python3
"""Validate example IAM policy invariants (not real provider enforcement)."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]/"docs"/"policies"
BUCKET="arn:aws:s3:::__BUCKET__"
PREFIX=BUCKET+"/__PREFIX__/"
DENY={"s3:DeleteObject","s3:DeleteObjectVersion","s3:PutBucketPolicy",
      "s3:PutBucketVersioning","s3:PutBucketObjectLockConfiguration","s3:BypassGovernanceRetention"}
def run(role,required):
    p=json.loads((ROOT/f"worm-{role}-iam.example.json").read_text())
    assert p["Version"]=="2012-10-17"
    allow=set();denied=set()
    for s in p["Statement"]:
        assert s["Effect"] in ("Allow","Deny")
        assert not ({"NotAction","NotResource"} & s.keys())
        actions=set(s["Action"])
        assert "*" not in actions and "s3:*" not in actions
        resources=s["Resource"]
        assert all(x==BUCKET or x.startswith(PREFIX) or (s["Effect"]=="Deny" and x==BUCKET+"/*") for x in resources)
        if s["Effect"]=="Allow": allow|=actions
        else:denied|=actions
    assert allow==required,(role,allow^required)
    assert DENY<=denied,(role,DENY-denied)
    if role=="writer":
        assert "s3:GetObject" not in allow and "s3:ListBucket" not in allow
        allowed_versions=[s for s in p["Statement"] if s["Effect"]=="Allow" and "s3:GetObjectVersion" in s["Action"]]
        assert len(allowed_versions)==1 and allowed_versions[0]["Resource"]==[PREFIX+"manifests/*"]
    else:
        assert not any(x.startswith("s3:Put") or x.startswith("s3:Delete") for x in allow)
        assert {"s3:PutObject","s3:PutObjectRetention"}<=denied
run("writer",{"s3:GetBucketVersioning","s3:GetBucketObjectLockConfiguration","s3:PutObject","s3:PutObjectRetention","s3:GetObjectRetention","s3:GetObjectVersion"})
run("recovery",{"s3:GetBucketVersioning","s3:GetBucketObjectLockConfiguration","s3:GetObjectRetention","s3:GetObjectVersion"})
print("PASS static WORM IAM least-privilege contract (provider IAM NOT certified)")
