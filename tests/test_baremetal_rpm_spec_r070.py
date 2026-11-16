#!/usr/bin/env python3
"""El9/El10 RPM spec must never rely on optional systemd package macros."""
import pathlib,unittest
SPEC=pathlib.Path(__file__).resolve().parents[1]/"packaging/linux/multi-backup-server.spec"

class NativeServerSpecTests(unittest.TestCase):
    def test_rpm_service_install_paths_are_explicit_and_in_filelist(self):
        s=SPEC.read_text()
        self.assertNotIn("%{_unitdir}",s)
        for name in ["multi-backup-server.service","multi-backup-snmp.service"]:
            self.assertIn(f"%{{buildroot}}/usr/lib/systemd/system/{name}",s)
            self.assertIn(f"\n/usr/lib/systemd/system/{name}\n",s)
        self.assertIn("%config(noreplace)",s)
        self.assertIn("%attr(0600,root,root)",s)
        self.assertIn("systemctl daemon-reload",s)

if __name__=="__main__":unittest.main()
