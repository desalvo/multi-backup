#!/bin/sh
# LOCAL-ONLY disposable demonstration, with a known password; NOT production.
set -eu
umask 077
mkdir -p /data/config /data/backups /data/exports /data/audit /data/demo-cloud/Projects /data/demo-source
if [ ! -s /data/config/admin_hash ]; then
  printf '%s\n' '$argon2id$v=19$m=19456,t=2,p=1$MXZ68eRL3w5+1bAGuy3QYw$bZWlM25B4hoCaST/0GDISpGLRR9Phq5OU5et0/1c2D0' > /data/config/admin_hash
fi
if [ ! -s /data/config/rclone.conf ]; then
  cat > /data/config/rclone.conf <<'EOF'
[demo]
type = alias
remote = /data/demo-cloud
EOF
fi
if [ ! -s /data/config/cloud-profiles.json ]; then
  cat > /data/config/cloud-profiles.json <<'EOF'
{"rclone_config":"/data/config/rclone.conf","profiles":[{"id":"demo","provider":"other","remote":"demo","prefix":"","allow_upload":true,"allow_delete":true}]}
EOF
fi
if [ ! -s /data/demo-cloud/Projects/readme.txt ]; then
  printf '%s\n' 'Multi Backup r218: REAL file in a disposable rclone alias, not a live cloud account.' > /data/demo-cloud/Projects/readme.txt
fi
if [ ! -s /data/demo-cloud/Projects/restore-policy.txt ]; then
  printf '%s\n' 'Encrypted .age objects cannot be deleted through this browser.' > /data/demo-cloud/Projects/restore-policy.txt
fi
if [ ! -s /data/demo-source/readme.txt ]; then
  printf '%s\n' 'Verified example backup source in the Multi Backup Rust chunk store.' > /data/demo-source/readme.txt
fi
if [ ! -s /data/config/age-identity.txt ]; then
  age-keygen -o /data/config/age-identity.txt >/dev/null 2>&1
fi
if [ ! -s /data/backups/.multi-backup-v1/config.json.age ]; then
  recipient="$(age-keygen -y /data/config/age-identity.txt)"
  multi-backup-native storage init --root /data/backups --recipient "$recipient"
fi
if [ ! -s /data/config/seeded ]; then
  printf '%s\n' '[{"namespace":"example","directory":"/data/demo-source"}]' > /data/config/sources.json
  multi-backup-native storage backup --root /data/backups --identity /data/config/age-identity.txt --label "Esempio backup cifrato" --sources-json /data/config/sources.json
  printf '%s\n' 'initialized' > /data/config/seeded
fi
exec /usr/local/bin/multi-backup-server
