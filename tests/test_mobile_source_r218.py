"""r218 source-level negative guard; native compile runs in separate mobile CI."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]

class MobileContracts(unittest.TestCase):
    def read(self, path):
        return (ROOT / path).read_text()

    def test_android_tls_and_no_release_signing(self):
        manifest = self.read("mobile/android/app/src/main/AndroidManifest.xml")
        gradle = self.read("mobile/android/app/build.gradle")
        source = self.read("mobile/android/app/src/main/java/eu/desalvo/multibackup/mobile/MainActivity.java")
        self.assertIn('usesCleartextTraffic="false"', manifest)
        self.assertIn('allowBackup="false"', manifest)
        self.assertIn('permission.INTERNET', manifest)
        self.assertNotIn("signingConfig signingConfigs.", gradle)
        self.assertIn('MIXED_CONTENT_NEVER_ALLOW', source)
        self.assertIn('handler.cancel()', source)
        self.assertIn('setAllowFileAccess(false)', source)
        self.assertIn('new URI("https"', source)
        self.assertNotIn("addJavascriptInterface(", source)

    def test_ios_tls_and_universal_device_family(self):
        project = self.read("mobile/ios/project.yml")
        source = self.read("mobile/ios/App/MultiBackupApp.swift")
        self.assertIn('TARGETED_DEVICE_FAMILY: "1,2"', project)
        self.assertIn("CODE_SIGNING_ALLOWED: NO", project)
        self.assertIn('url.scheme?.lowercased() == "https"', source)
        self.assertIn('WKWebView', source)
        self.assertIn('decisionHandler(.cancel)', source)
        self.assertNotIn('NSAllowsArbitraryLoads', self.read("mobile/ios/App/Info.plist"))

    def test_optional_mobile_workflow_is_outside_rc(self):
        mobile = self.read(".github/workflows/mobile-unsigned-preview.yml")
        rc = self.read(".github/workflows/rc-preflight-r217.yml")
        self.assertIn("app-release-unsigned.apk", mobile)
        self.assertIn("CODE_SIGNING_ALLOWED=NO", mobile)
        self.assertIn("upload-artifact@v4", mobile)
        self.assertNotIn("mobile-unsigned-preview", rc)
        self.assertNotIn("mobile-contract", rc)
        self.assertNotIn("ios-ipados-unsigned", rc)

    def test_console_has_touch_navigation(self):
        css = self.read("rust-server/public/app.css")
        html = self.read("rust-server/public/index.html")
        self.assertIn('id="section-jump"', html)
        self.assertIn('mobile-nav', css)
        self.assertIn('min-height:44px', css)

if __name__ == "__main__":
    unittest.main()
