"""r215 READY is per proven operation, not provider brand or generic executor."""
import copy
import importlib.util
from pathlib import Path
from unittest import TestCase

f=Path(__file__).resolve().parents[1]/"scripts/ci/rc-preflight-r215.py"
spec=importlib.util.spec_from_file_location("rc215",f)
rc=importlib.util.module_from_spec(spec)
spec.loader.exec_module(rc)
SHA="a"*40
RUN="44199999999"

def fixture():
    plugins=[dict(id=f"unqualified-{i}",backup_input="planned",
                  backup_output="planned",restore_input="planned",
                  restore_output="planned",ready_scope=None) for i in range(33)]
    for i,(name,(roles,scope)) in enumerate(rc.READY_ROLES.items()):
        plugins[i].update(id=name,ready_scope=scope+"; unqualified roles forbidden",
                          **dict(zip(rc.FIELDS,roles)))
    def plan(mode):
        return dict(mode=mode,source_plugin="posix",destination_plugin="posix",
                    source_stage="ready",destination_stage="ready",executable=True,
                    preview_executable=False,confirmation_required_before_writes=True,
                    reason="scoped content-only quiesced regular file data")
    labs=dict(source_sha=SHA,actions_run_id=int(RUN))
    labs.update({k:"success" for k in rc.MANDATORY_LABS})
    return plugins,plan("backup"),plan("restore"),labs

class ReadyContract(TestCase):
    def test_nine_bounded_ready_and_five_field_labs(self):
        result=rc.validate(*fixture()[:3],SHA,RUN,fixture()[3])
        self.assertEqual(result["scoped_ready_count"],9)
        self.assertFalse(result["all_providers_ready"])
        self.assertFalse(result["rc_publication_authorized"])
        self.assertIn("anti-ransomware-vault",result["scopes"])

    def test_fail_closed_on_every_wrong_provider_role(self):
        c,b,r,g=fixture()
        for name in rc.READY_ROLES:
            idx=next(i for i,p in enumerate(c) if p["id"]==name)
            for role in rc.FIELDS:
                original=c[idx][role]
                wrong="preview" if original=="ready" else "ready"
                bad=copy.deepcopy(c)
                bad[idx][role]=wrong
                with self.subTest(name=name,role=role),self.assertRaises(rc.PromotionError):
                    rc.validate(bad,b,r,SHA,RUN,g)

    def test_no_accidental_promotion_of_preview_provider(self):
        c,b,r,g=fixture()
        c[13]["backup_input"]="ready"
        c[13]["ready_scope"]="fake_complete; never"
        with self.assertRaises(rc.PromotionError):
            rc.validate(c,b,r,SHA,RUN,g)

    def test_skipped_or_wrong_source_vendor_laboratory_denies_rc(self):
        for field in rc.MANDATORY_LABS:
            c,b,r,g=fixture()
            g[field]="skipped"
            with self.subTest(field=field),self.assertRaises(rc.PromotionError):
                rc.validate(c,b,r,SHA,RUN,g)
        for field,value in (("source_sha","b"*40),("actions_run_id",123),
                            ("minio_worm_iam","cancelled")):
            c,b,r,g=fixture()
            g[field]=value
            with self.subTest(field=field),self.assertRaises(rc.PromotionError):
                rc.validate(c,b,r,SHA,RUN,g)

    def test_not_a_generic_provider_composer(self):
        c,b,r,g=fixture()
        for field,value in (("source_plugin","rbd"),("destination_plugin","iscsi"),
                            ("executable",False),("preview_executable",True)):
            bad=dict(b,**{field:value})
            with self.subTest(field=field),self.assertRaises(rc.PromotionError):
                rc.validate(c,bad,r,SHA,RUN,g)

    def test_bad_catalog_or_scope_refused(self):
        c,b,r,g=fixture()
        with self.assertRaises(rc.PromotionError):
            rc.validate(c[:-1],b,r,SHA,RUN,g)
        c,b,r,g=fixture()
        c[1]["ready_scope"]="minio_version_pinned_single_object_sha256_age_import"
        with self.assertRaises(rc.PromotionError):
            rc.validate(c,b,r,SHA,RUN,g)
        c,b,r,g=fixture()
        c[0]["ready_scope"]=None
        with self.assertRaises(rc.PromotionError):
            rc.validate(c,b,r,SHA,RUN,g)
