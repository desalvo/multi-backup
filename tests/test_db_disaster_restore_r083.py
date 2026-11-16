"""Fail-closed, fully isolated shell-drill for offline DR version-pin forwarding.

Only synthetic executables and temporary files are used. This is NOT a
PostgreSQL, S3 Object Lock, age cryptography or HA field certification.
"""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "db-disaster-restore.sh"


class DisasterRecoveryShellDrill(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.stage = self.root / "stage"
        self.stage.mkdir(mode=0o700)
        self.key = self.root / "identity.txt"
        self.key.write_text("SYNTHETIC KEY NEVER USED FOR REAL DECRYPTION\n")
        self.log = self.root / "calls.log"
        self.log.touch()
        self._stub("systemctl", "exit 1\n")
        self._stub("psql", '''
if [[ "$*" == *pg_is_in_recovery* ]]; then
  printf '%s\\n' "${DR_TEST_ROLE:-f}"
else
  printf '%s\\n' "${DR_TEST_TABLES:-0}"
fi
''')
        self._stub("multi-backup-native", '''
printf '%s\\n' "$*" >> "$DR_TEST_LOG"
if [ "$1" = 's3-vault' ] && [ "$2" = 'restore' ]; then
  previous=''
  for arg in "$@"; do
    if [ "$previous" = '--destination' ]; then
      mkdir -p "$arg"
      case "$arg" in
        */db) printf '%s\\n' 'MOCK encrypted dump' > "$arg/internal-postgresql.dump.age" ;;
        */data) printf '%s\\n' 'MOCK data' > "$arg/example.data" ;;
      esac
    fi
    previous="$arg"
  done
fi
''')
        self._stub("age", "printf '%s\\n' 'MOCK decrypted stream'\n")
        self._stub("pg_restore", 'cat >/dev/null\nprintf "%s\\n" "pg_restore" >> "$DR_TEST_LOG"\n')
        self.env = os.environ.copy()
        self.env.update({
            "PATH": f"{self.bin}:{os.environ['PATH']}",
            "MULTI_BACKUP_DR_CONFIRMATION": "ISOLATED-NEW-EMPTY-CLUSTER",
            "MULTI_BACKUP_DR_DB_TARGET_DSN": "postgresql://mock@localhost/mock",
            "MULTI_BACKUP_DR_DB_WORM_CONFIG": str(self.root / "db-s3.json"),
            "MULTI_BACKUP_DR_DB_SNAPSHOT_ID": "db-083",
            "MULTI_BACKUP_DR_AGE_IDENTITY": str(self.key),
            "MULTI_BACKUP_DR_STAGE_ROOT": str(self.stage),
            "DR_TEST_LOG": str(self.log),
        })
        for field in ("MULTI_BACKUP_DR_DATA_ID", "MULTI_BACKUP_DR_DATA_WORM_CONFIG",
                      "MULTI_BACKUP_DR_DATA_MANIFEST_VERSION", "MULTI_BACKUP_DR_DB_MANIFEST_VERSION",
                      "DR_TEST_ROLE", "DR_TEST_TABLES"):
            self.env.pop(field, None)

    def _stub(self, name, body):
        target = self.bin / name
        target.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body)
        target.chmod(0o700)

    def run_dr(self):
        return subprocess.run(["bash", str(SCRIPT)], env=self.env,
                              text=True, capture_output=True, timeout=15, check=False)

    def calls(self):
        return self.log.read_text().splitlines()

    def test_without_confirmation_never_contacts_storage(self):
        self.env.pop("MULTI_BACKUP_DR_CONFIRMATION")
        result = self.run_dr()
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_version_pins_forwarded_to_both_verification_and_restore(self):
        self.env.update({
            "MULTI_BACKUP_DR_DB_MANIFEST_VERSION": "version-db-123",
            "MULTI_BACKUP_DR_DATA_ID": "data-083",
            "MULTI_BACKUP_DR_DATA_WORM_CONFIG": str(self.root / "data-s3.json"),
            "MULTI_BACKUP_DR_DATA_MANIFEST_VERSION": "version-data-456",
        })
        result = self.run_dr()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        for action in ("verify", "restore"):
            db = [x for x in calls if x.startswith(f"s3-vault {action} ") and "--id db-083" in x]
            data = [x for x in calls if x.startswith(f"s3-vault {action} ") and "--id data-083" in x]
            self.assertEqual(len(db), 1, calls)
            self.assertEqual(len(data), 1, calls)
            self.assertIn("--manifest-version version-db-123", db[0])
            self.assertIn("--manifest-version version-data-456", data[0])
        self.assertEqual(calls[-1], "pg_restore")

    def test_absent_pin_uses_default_read_without_inventing_version(self):
        result = self.run_dr()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(any("--manifest-version" in x for x in self.calls()))

    def test_nonempty_stage_refused_before_s3_access(self):
        (self.stage / "already-present").write_text("data")
        result = self.run_dr()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("staging root must be empty", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_existing_database_objects_refused_before_s3_access(self):
        self.env["DR_TEST_TABLES"] = "1"
        result = self.run_dr()
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_standby_is_refused_before_s3_access(self):
        self.env["DR_TEST_ROLE"] = "t"
        result = self.run_dr()
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(self.calls(), [])

    def test_data_version_without_data_snapshot_is_refused(self):
        self.env["MULTI_BACKUP_DR_DATA_MANIFEST_VERSION"] = "not-paired"
        result = self.run_dr()
        self.assertEqual(result.returncode, 3, result.stderr)
        self.assertEqual(self.calls(), [])


if __name__ == "__main__":
    unittest.main()
