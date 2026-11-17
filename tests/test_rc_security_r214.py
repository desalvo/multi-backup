"""Security regressions for true baremetal HA witness and KVM lab XML."""
from __future__ import annotations
import base64
import importlib.util
from pathlib import Path
import re
from unittest import TestCase
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
def load(path,label):
    spec=importlib.util.spec_from_file_location(label,ROOT / path)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class SecurityControls(TestCase):
    def test_hardware_ha_sql_uses_quoted_psql_variable_not_shell_concat(self):
        mod=load("scripts/ci/ha-three-physical-redfish-r204.py","ha_redfish")
        nonce="a"*32
        statements=[
            "INSERT INTO public.mb_physical_witness VALUES (:'witness_id','acknowledged-before-power-off')",
            "SELECT count(*) FROM public.mb_physical_witness WHERE id=:'witness_id'",
        ]
        for query in statements:
            with self.subTest(query=query), patch.object(mod,"ssh",return_value="1") as ssh:
                self.assertEqual(mod.sql("lab-host.example",query,witness=nonce),"1")
                sent=ssh.call_args.args[1]
                self.assertIn("-v witness_id="+nonce,sent)
                matched=re.search(r"printf '%s' '([A-Za-z0-9+/=]+)'",sent)
                self.assertIsNotNone(matched)
                decoded=base64.b64decode(matched.group(1)).decode("ascii")
                self.assertEqual(decoded,query+"\n")
                # SQL is transported only as base64, never interpolated in shell script.
                self.assertNotIn(query,sent)
        for bad in ["' OR 1=1;--","a"*31,"x"*32,"a"*32+"'"]:
            with self.subTest(bad=bad),self.assertRaises(RuntimeError):
                mod.sql("lab-host.example",statements[1],witness=bad)
        with self.assertRaises(RuntimeError):
            mod.sql("lab-host.example","DROP TABLE users;")
        with self.assertRaises(RuntimeError):
            mod.sql("lab-host.example",statements[1])

    def test_libvirt_xml_rejects_entity_expansion(self):
        from defusedxml import ElementTree as xml
        from defusedxml.common import EntitiesForbidden
        malicious='<!DOCTYPE foo [<!ENTITY x "evil">]><network><name>&x;</name></network>'
        with self.assertRaises(EntitiesForbidden):
            xml.fromstring(malicious)
        root=xml.fromstring('<network><name>mb-r213-isolated</name></network>')
        self.assertEqual(root.findtext("name"),"mb-r213-isolated")
        self.assertIsNone(root.find("forward"))
