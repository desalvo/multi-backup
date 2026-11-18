"""r216 refuses false READY, skipped real vendor labs and unsupported installers."""
import copy
import importlib.util
from pathlib import Path
import unittest

PATH=Path(__file__).resolve().parents[1]/"scripts/ci/rc-preflight-r216.py"
spec=importlib.util.spec_from_file_location("rc216",PATH)
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
SHA="a"*40
RUN="44222222222"

def fixture():
    catalog=[dict(id=f"not-qualified-{i}", backup_input="planned",
                  backup_output="planned",restore_input="planned",
                  restore_output="planned",ready_scope=None) for i in range(33)]
    for i,(id,(roles,scope)) in enumerate(module.READY_ROLES.items()):
        catalog[i].update(id=id,ready_scope=scope+"; no extra provider operations",
                          **dict(zip(module.FIELDS,roles)))
    def plan(mode):
        return dict(mode=mode,source_plugin="posix",destination_plugin="posix",
                    source_stage="ready",destination_stage="ready",executable=True,
                    preview_executable=False,confirmation_required_before_writes=True,
                    reason="READY content-only regular-file recovery")
    lab=dict(source_sha=SHA,actions_run_id=int(RUN))
    lab.update({s:"success" for s in module.MANDATORY_LABS})
    packages=dict(source_sha=SHA,actions_run_id=int(RUN))
    packages.update({s:"success" for s in module.MANDATORY_INSTALLERS})
    return catalog,plan("backup"),plan("restore"),lab,packages

def qualified(inputs):
    c,b,r,lab,packages=inputs
    return module.validate(c,b,r,SHA,RUN,lab,packages)

class GateTests(unittest.TestCase):
    def test_honest_ready_and_installer_evidence(self):
        result=qualified(fixture())
        self.assertEqual(result["scoped_ready_count"],13)
        self.assertEqual(result["plugin_count"],33)
        self.assertFalse(result["all_providers_ready"])
        self.assertFalse(result["rc_publication_authorized"])
        self.assertFalse(result["macos_signed_notarized_certified"])

    def test_each_role_must_match_compiled_provider_and_not_generic_dispatcher(self):
        for id in module.READY_ROLES:
            for role in module.FIELDS:
                inputs=fixture()
                c=inputs[0]
                item=next(p for p in c if p["id"]==id)
                item[role]="preview" if item[role]=="ready" else "ready"
                with self.subTest(id=id,role=role),self.assertRaises(module.PromotionError):
                    qualified(inputs)
        for change in ({"source_plugin":"s3"},{"destination_plugin":"webdav"},
                       {"executable":False},{"preview_executable":True}):
            inputs=fixture()
            inputs[1].update(change)
            with self.subTest(change=change),self.assertRaises(module.PromotionError):
                qualified(inputs)

    def test_no_production_claim_on_missing_real_vendor_lab(self):
        for name in module.MANDATORY_LABS:
            for status in ("skipped","cancelled","failure",""):
                inputs=fixture()
                inputs[3][name]=status
                with self.subTest(name=name,status=status),self.assertRaises(module.PromotionError):
                    qualified(inputs)

    def test_no_unverified_native_installer_or_source_mismatch(self):
        for name in module.MANDATORY_INSTALLERS:
            for status in ("skipped","failure","cancelled"):
                inputs=fixture()
                inputs[4][name]=status
                with self.subTest(name=name,status=status),self.assertRaises(module.PromotionError):
                    qualified(inputs)
        for evidence_index in (3,4):
            inputs=fixture()
            inputs[evidence_index]["source_sha"]="b"*40
            with self.assertRaises(module.PromotionError):
                qualified(inputs)

    def test_fake_extra_ready_refused_and_empty_catalog_denied(self):
        inputs=fixture()
        inputs[0][21]["backup_input"]="ready"
        inputs[0][21]["ready_scope"]="fake; no"
        with self.assertRaises(module.PromotionError):
            qualified(inputs)
        inputs=fixture()
        inputs[0].pop()
        with self.assertRaises(module.PromotionError):
            qualified(inputs)

if __name__=="__main__":
    unittest.main()
