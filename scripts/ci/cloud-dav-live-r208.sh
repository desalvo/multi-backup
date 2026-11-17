#!/usr/bin/env bash
# Real ownCloud Classic 10.16.7 OR Nextcloud Apache 32 HTTPS WebDAV lab.
# Dedicated disposable Docker server and TLS CA. NO generic rclone stand-in.
set -Eeuo pipefail
umask 077
test "$RUNNER_ENVIRONMENT" = github-hosted
PROVIDER="$1"
case "$PROVIDER" in
 owncloud) IMAGE=owncloud/server:10.16.7; PORT=19120; HTTP=18120;;
 nextcloud) IMAGE=nextcloud:32-apache; PORT=19121; HTTP=18121;;
 *) echo 'Only genuine owncloud|nextcloud Docker provider accepted' >&2;exit 1;;
esac
CLI=rust/target/debug/multi-backup-native
test -x "$CLI"
W=$(mktemp -d /tmp/mb-cloud-dav.XXXXXXXX)
NAME="mb-$PROVIDER-$(openssl rand -hex 5)"
PASSWORD=$(openssl rand -hex 18)
cleanup(){
 set +e
 sudo nginx -s stop -c "$W/nginx.conf" -p "$W/" >/dev/null 2>&1 || :
 docker rm -f "$NAME" >/dev/null 2>&1 || :
 sudo rm -rf -- "$W"
}
trap cleanup EXIT
mkdir -p "$W/source/nested"
printf 'Real %s authenticated DAV ciphertext\n' "$PROVIDER" > "$W/source/nested/message.txt"
dd if=/dev/urandom of="$W/source/blob.bin" bs=1024 count=512 status=none
openssl req -x509 -newkey rsa:3072 -nodes -sha256 -days 2 \
 -subj '/CN=Multi Backup disposable cloud DAV CA' \
 -addext 'basicConstraints=critical,CA:TRUE' \
 -addext 'keyUsage=critical,keyCertSign,cRLSign' \
 -keyout "$W/ca.key" -out "$W/ca.crt" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -sha256 -subj /CN=localhost \
 -keyout "$W/leaf.key" -out "$W/leaf.csr" >/dev/null 2>&1
cat > "$W/leaf.ext" <<'CERT'
subjectAltName=DNS:localhost,IP:127.0.0.1
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
CERT
openssl x509 -req -in "$W/leaf.csr" -CA "$W/ca.crt" \
 -CAkey "$W/ca.key" -CAcreateserial -sha256 -days 1 \
 -extfile "$W/leaf.ext" -out "$W/leaf.crt" >/dev/null 2>&1
openssl verify -purpose sslserver -CAfile "$W/ca.crt" "$W/leaf.crt"
sudo install -m 0644 "$W/ca.crt" "/usr/local/share/ca-certificates/mb-cloud-$PROVIDER.crt"
sudo update-ca-certificates >/dev/null
cat > "$W/nginx.conf" <<EOF
worker_processes 1;
pid $W/nginx.pid;
error_log $W/nginx.error warn;
events { worker_connections 128; }
http {
  client_max_body_size 120m;
  server {
    listen 127.0.0.1:$PORT ssl;
    server_name 127.0.0.1;
    ssl_certificate $W/leaf.crt;
    ssl_certificate_key $W/leaf.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    location / {
      proxy_pass http://127.0.0.1:$HTTP;
      proxy_http_version 1.1;
      proxy_set_header Host 127.0.0.1:$PORT;
      proxy_set_header X-Forwarded-Proto https;
      proxy_set_header X-Real-IP 127.0.0.1;
      proxy_request_buffering off;
      proxy_read_timeout 180s;
    }
  }
}
EOF
sudo nginx -c "$W/nginx.conf" -p "$W/"
if test "$PROVIDER" = owncloud;then
 docker run -d --name "$NAME" -p "127.0.0.1:$HTTP:8080" \
  -e OWNCLOUD_DOMAIN="127.0.0.1:$PORT" \
  -e OWNCLOUD_TRUSTED_DOMAINS="127.0.0.1,localhost" \
  -e OWNCLOUD_DB_TYPE=sqlite \
  -e OWNCLOUD_ADMIN_USERNAME=ci -e OWNCLOUD_ADMIN_PASSWORD="$PASSWORD" \
  "$IMAGE" >/dev/null
 DAV="https://127.0.0.1:$PORT/remote.php/webdav/"
else
 docker run -d --name "$NAME" -p "127.0.0.1:$HTTP:80" \
  -e NEXTCLOUD_ADMIN_USER=ci -e NEXTCLOUD_ADMIN_PASSWORD="$PASSWORD" \
  -e SQLITE_DATABASE=nextcloud \
  -e NEXTCLOUD_TRUSTED_DOMAINS="127.0.0.1 localhost" \
  "$IMAGE" >/dev/null
 DAV="https://127.0.0.1:$PORT/remote.php/dav/files/ci/"
fi
STATUS="https://127.0.0.1:$PORT/status.php"
READY=0
for i in $(seq 1 90);do
 if curl -fsS --connect-timeout 4 --max-time 8 "$STATUS" \
    >"$W/status.json" 2>/dev/null &&
    jq -e --arg provider "$PROVIDER" \
     '.installed==true and .maintenance!=true and (.productname|ascii_downcase)==$provider' \
     "$W/status.json" >/dev/null 2>&1;then READY=1;break;fi
 if test "$(docker inspect -f '{{.State.Running}}' "$NAME")" != true;then
  echo 'FAIL: cloud provider container exited during initialization' >&2;exit 1
 fi
 sleep 5
done
if test "$READY" != 1;then
 echo 'FAIL: genuine cloud provider did not publish expected healthy status.php' >&2
 jq -r '{installed,productname,maintenance}' "$W/status.json" 2>/dev/null || :
 docker logs --tail 20 "$NAME" 2>&1 | grep -Ei '(ERROR|FAIL|fatal|database)' | tail -8 >&2 || :
 exit 1
fi
# Probe actual provider-authenticated DAV user collection (not plain WebDAV mock).
curl -fsS -u "ci:$PASSWORD" -X PROPFIND -H 'Depth: 0' \
 --output "$W/dav-prop.xml" "$DAV" >/dev/null
grep -Eiq 'multistatus|propstat' "$W/dav-prop.xml"
if curl -fsS -X PROPFIND -H 'Depth: 0' "$DAV" >/dev/null 2>&1;then
 echo 'FAIL: unauthenticated provider account exposed DAV files' >&2;exit 1
fi
jq -n --arg provider "$PROVIDER" --arg endpoint "$DAV" \
 --arg status "$STATUS" \
 '{provider:$provider,endpoint:$endpoint,status_url:$status,username:"ci"}' >"$W/owner.json"
export MULTI_BACKUP_WEBDAV_PASSWORD="$PASSWORD"
age-keygen -o "$W/key" >/dev/null 2>&1
RECIPIENT=$(age-keygen -y "$W/key")
"$CLI" plugins cloud-dav-backup --config "$W/owner.json" \
 --source "$W/source" --filename "cloud-r208.tar.gz.age" \
 --recipient "$RECIPIENT" --identity "$W/key" --json > "$W/backup.json"
jq -e --arg provider "$PROVIDER" \
 '.provider==$provider and .vendor_status_verified and .encrypted and .create_only and .independent_remote_readback' \
 "$W/backup.json"
"$CLI" plugins cloud-dav-restore --config "$W/owner.json" \
 --filename cloud-r208.tar.gz.age --identity "$W/key" \
 --destination "$W/restored" --json > "$W/restore.json"
jq -e --arg provider "$PROVIDER" \
 '.provider==$provider and .age_verified and .create_only_local_restore and .restored_members>0' \
 "$W/restore.json"
cmp "$W/source/blob.bin" "$W/restored/multi-backup/webdav-native/blob.bin"
cmp "$W/source/nested/message.txt" "$W/restored/multi-backup/webdav-native/nested/message.txt"
if "$CLI" plugins cloud-dav-backup --config "$W/owner.json" \
 --source "$W/source" --filename cloud-r208.tar.gz.age \
 --recipient "$RECIPIENT" --identity "$W/key" >/dev/null 2>&1;then
 echo 'FAIL: vendor WebDAV ciphertext overwritten' >&2;exit 1
fi
age-keygen -o "$W/wrongkey" >/dev/null 2>&1
if "$CLI" plugins cloud-dav-restore --config "$W/owner.json" \
 --filename cloud-r208.tar.gz.age --identity "$W/wrongkey" \
 --destination "$W/wrongrestore" >/dev/null 2>&1;then
 echo 'FAIL: unrelated age identity restored provider ciphertext' >&2;exit 1
fi
echo "PASS real $PROVIDER Docker HTTP backend through strict HTTPS CA-verified reverse proxy, native Rust age ciphertext PUT/GET, SHA, no overwrite, independent local decrypt/restore"
echo "NOT CERTIFIED provider source file revisions/shares/ACL, calendar/contact app metadata, enterprise HA or distributed transaction snapshots"
