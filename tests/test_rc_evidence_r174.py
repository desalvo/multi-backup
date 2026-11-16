"""r174 RC candidate must fail closed on stale, skipped or incomplete evidence."""
import importlib.util
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[1]
SRC=ROOT/"scripts/ci/validate-rc-evidence.py"
SPEC=importlib.util.spec_from_file_location("rc_evidence", SRC)
rc=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rc)

def fixtures():
    run={"id":rc.RUN_ID,"head_sha":rc.SOURCE_SHA,
         "status":"completed","conclusion":"success"}
    jobs={"jobs":[{"name":n,"conclusion":"success"} for n in sorted(rc.EXPECTED_SUCCESS)]
          +[{"name":n,"conclusion":"skipped"} for n in sorted(rc.PREVIEW_SKIP)]}
    artifacts={"artifacts":[{"name":n,"expired":False,"size_in_bytes":100,
             "workflow_run":{"head_sha":rc.SOURCE_SHA}}
             for n in sorted(rc.required_artifacts()|{"native-gui-and-about-preview"})]}
    return run,jobs,artifacts

class CandidateEvidenceTests(unittest.TestCase):
    def test_qualified_source_is_accepted(self):
        self.assertEqual(rc.validate(*fixtures())["required_jobs"],32)
    def test_wrong_sha_or_failed_run_is_denied(self):
        run,jobs,artifacts=fixtures()
        run["head_sha"]="0"*40
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
        run["head_sha"]=rc.SOURCE_SHA
        run["conclusion"]="failure"
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
    def test_one_failed_or_skipped_mandatory_job_denies_rc(self):
        run,jobs,artifacts=fixtures()
        jobs["jobs"][0]["conclusion"]="skipped"
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
    def test_missing_agent_architecture_denies_rc(self):
        run,jobs,artifacts=fixtures()
        artifacts["artifacts"]=[a for a in artifacts["artifacts"]
                                if a["name"]!="rust-agent-macos-arm64-preview"]
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
    def test_duplicate_artifact_denies_rc(self):
        run,jobs,artifacts=fixtures()
        artifacts["artifacts"].append(artifacts["artifacts"][0].copy())
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
    def test_unknown_artifact_or_expired_is_denied(self):
        run,jobs,artifacts=fixtures()
        artifacts["artifacts"][0]["expired"]=True
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
        artifacts["artifacts"][0]["expired"]=False
        artifacts["artifacts"].append({"name":"legacy-python-installer","expired":False,"size_in_bytes":1})
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)
    def test_publisher_and_legacy_job_must_remain_skipped(self):
        run,jobs,artifacts=fixtures()
        for j in jobs["jobs"]:
            if j["name"]=="Push Multi Backup OCI image to Docker Hub":
                j["conclusion"]="success"
        with self.assertRaises(ValueError):rc.validate(run,jobs,artifacts)

if __name__=="__main__":
    unittest.main()
