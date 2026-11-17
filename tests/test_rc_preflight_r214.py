"""Negative RC promotion contract tests: never mistake provider preview for Ready."""
import copy
import importlib.util
import pathlib
import unittest

FILE=pathlib.Path(__file__).resolve().parents[1]/"scripts/ci/rc-preflight-r214.py"
spec=importlib.util.spec_from_file_location("mb_rc_preflight",FILE)
mod=importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

SHA="a"*40
RUN="44155555555"

def plugin(i):
    return dict(id=f"p{i}", backup_input="preview",backup_output="planned",
                restore_input="planned",restore_output="preview",ready_scope=None)
def fixture():
    plugins=[plugin(i) for i in range(33)]
    plugins[0].update(id="posix",backup_input="ready",backup_output="ready",
                      restore_input="ready",restore_output="ready",
                      ready_scope="content_only_quiesced_regular_files; no ACL/xattr")
    def plan(mode):
        return dict(mode=mode,source_plugin="posix",destination_plugin="posix",
                    source_stage="ready",destination_stage="ready",
                    executable=True,preview_executable=False,
                    confirmation_required_before_writes=True,
                    reason="READY quiesced content-only regular file paths")
    return plugins,plan("backup"),plan("restore")

class RcTest(unittest.TestCase):
    def test_positive_scoped_ready(self):
        data=fixture()
        result=mod.validate(*data,SHA,RUN)
        self.assertEqual(result["ready_scoped_plugins"],["posix"])
        self.assertFalse(result["rc_publication_authorized"])
        self.assertFalse(result["all_plugins_ready"])

    def test_extra_unqualified_provider_refused(self):
        a,b,r=fixture()
        a[1]["backup_input"]="ready"
        a[1]["ready_scope"]="false-full-certification"
        with self.assertRaises(mod.PromotionError):
            mod.validate(a,b,r,SHA,RUN)

    def test_catalog_missing_entry_refused(self):
        a,b,r=fixture()
        with self.assertRaises(mod.PromotionError):
            mod.validate(a[:-1],b,r,SHA,RUN)

    def test_wrong_sha_and_run_refused(self):
        data=fixture()
        for sha,run in (("deadbeef",RUN),(SHA,"123"),(SHA,"0")):
            with self.assertRaises(mod.PromotionError):
                mod.validate(*data,sha,run)

    def test_false_executable_and_preview_refused(self):
        for name,value in (("executable",False),
                           ("preview_executable",True),
                           ("confirmation_required_before_writes",False),
                           ("source_stage","preview")):
            data=fixture()
            bad=copy.deepcopy(data[1])
            bad[name]=value
            with self.subTest(field=name),self.assertRaises(mod.PromotionError):
                mod.validate(data[0],bad,data[2],SHA,RUN)

    def test_false_restore_refused(self):
        data=fixture()
        bad=copy.deepcopy(data[2])
        bad["source_plugin"]="s3"
        with self.assertRaises(mod.PromotionError):
            mod.validate(data[0],data[1],bad,SHA,RUN)

if __name__=="__main__":
    unittest.main()
