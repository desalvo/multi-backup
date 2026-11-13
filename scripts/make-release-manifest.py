"""Create inspectable SHA256SUMS.json for public release assets."""
from __future__ import annotations
import datetime as dt
import hashlib
import json
import sys
from pathlib import Path


def main(argv):
    folder=Path(argv[1]);channel=argv[2];commit=argv[3]
    if channel not in ('early-release','stable'):raise ValueError('Invalid channel')
    assets=[]
    for path in sorted(folder.iterdir()):
        if path.is_file() and path.name!='release-manifest.json':
            assets.append({'name':path.name,'bytes':path.stat().st_size,
                           'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
    (folder/'release-manifest.json').write_text(json.dumps({
        'schema':1,'channel':channel,'commit':commit,'published_utc':dt.datetime.now(dt.timezone.utc).isoformat(),
        'assets':assets},indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':main(sys.argv)
