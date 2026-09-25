"""确定性合成数据库验证；不读取真实会话，不访问模型/GUI。"""
import hashlib
import importlib.util
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("audit-computer-use.py")
spec = importlib.util.spec_from_file_location("cu_audit", SCRIPT)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="audit-fixture-", dir=SCRIPT.parent)
        self.addCleanup(self.folder.cleanup)
        self.db = Path(self.folder.name) / "synthetic.sqlite3"
        self.conn = sqlite3.connect(self.db)
        self.addCleanup(self.conn.close)
        self.conn.executescript("""
        CREATE TABLE computer_use_runs(call_id TEXT,turn_id TEXT,session_id TEXT,chat_room_id TEXT,surface TEXT,state TEXT,action_count INTEGER,terminal_result_json TEXT,created_at_ms INTEGER,updated_at_ms INTEGER);
        CREATE TABLE computer_use_steps(run_id TEXT,step_index INTEGER,observation_generation INTEGER,action_type TEXT,normalized_target TEXT,status TEXT,error_code TEXT,before_evidence_ref TEXT,after_evidence_ref TEXT,visible_progress INTEGER,started_at_ms INTEGER,completed_at_ms INTEGER);
        CREATE TABLE computer_use_step_details(run_id TEXT,step_index INTEGER,action_json TEXT);
        CREATE TABLE computer_use_planner_diagnostics(id INTEGER,call_id TEXT,turn_id TEXT,room_id TEXT,session_id TEXT,request_kind TEXT,observation_generation INTEGER,model TEXT,error_code TEXT,response_json TEXT,started_at_ms INTEGER,completed_at_ms INTEGER);
        CREATE TABLE chat_usage_events(id INTEGER,room_id TEXT,session_id TEXT,created_at INTEGER,input_tokens INTEGER,output_tokens INTEGER,cache_read_tokens INTEGER,cache_write_tokens INTEGER,turn_id TEXT,call_id TEXT,request_kind TEXT);
        """)
        self.secret = "NEVER-PUBLISH-USER-TEXT-API-KEY-RAW-THINKING"
        self.evidence = json.dumps(["screenshot:C:\\Users\\private-user\\evidence.png:sha256=" + "a" * 64 + ":760x460", "image_changed:true", self.secret])
        terminal = json.dumps({"status":"failed","goal_achieved":False,"attempts":1,"steps_completed":1,"summary":self.secret,"error":{"code":"invalid_plan","message":self.secret},"evidence":json.loads(self.evidence)})
        self.conn.execute("INSERT INTO computer_use_runs VALUES(?,?,?,?,?,?,?,?,?,?)", ("real-call-17","real-turn-3","real-session-9","private-room","desktop","failed",1,terminal,1000,2200))
        self.conn.execute("INSERT INTO computer_use_steps VALUES(?,?,?,?,?,?,?,?,?,?,?,?)", ("real-call-17",0,3,"drag","window-canvas:abc123","completed_unverified",None,self.evidence,self.evidence,1,1300,1900))
        action = {"kind":"drag","target":"window-canvas:abc123","arguments":{"points":[[0,0],[0.5,0.7],[1,1]],"duration_ms":400,"text":self.secret,"data_url":"data:image/png;base64,"+self.secret}}
        self.conn.execute("INSERT INTO computer_use_step_details VALUES(?,?,?)",("real-call-17",0,json.dumps(action)))
        self.conn.execute("INSERT INTO computer_use_planner_diagnostics VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",(1,"real-call-17","real-turn-3","private-room","real-session-9","computer_use_planning",3,r"C:\Users\private-user\model","invalid_plan",json.dumps({"action":"click","thinking":self.secret}),1100,1250))
        self.conn.execute("INSERT INTO chat_usage_events VALUES(?,?,?,?,?,?,?,?,?,?,?)",(1,"private-room","real-session-9",1250,100,20,10,5,"real-turn-3","real-call-17","computer_use_planning"))
        self.conn.execute("INSERT INTO chat_usage_events VALUES(?,?,?,?,?,?,?,?,?,?,?)",(2,"private-room","real-session-9",2100,50,10,0,0,None,None,None))
        self.conn.commit()

    def test_shapes_timing_usage_and_diagnostic_purpose(self):
        local, public = audit.build(self.db)
        step = local["runs"][0]["steps"][0]
        self.assertEqual(step["action_type"],"drag")
        self.assertEqual(step["target_kind"],"window_canvas")
        self.assertEqual(step["action"]["point_count"],3)
        self.assertEqual(step["timing"]["elapsed_ms"],600)
        self.assertEqual(public["summary"]["usage_totals"]["input_tokens"],150)
        self.assertEqual(public["summary"]["cu_usage_totals"]["input_tokens"],100)
        self.assertEqual(public["planner_diagnostics"][0]["request_kind"],"computer_use_planning")
        self.assertEqual(public["summary"]["diagnostic_errors"],{"invalid_plan":1})
        self.assertIn("local_path",local["runs"][0]["steps"][0]["before_evidence"][0])

    def test_public_excludes_identity_body_image_and_absolute_paths(self):
        local, public = audit.build(self.db)
        encoded = json.dumps(public)
        for denied in [self.secret,"real-call-17","real-turn-3","real-session-9","private-room","private-user","window-canvas:abc123","C:\\\\Users"]:
            self.assertNotIn(denied,encoded)
        self.assertNotIn(self.secret,json.dumps(local))
        self.assertNotIn("data:image/png;base64",encoded)
        self.assertEqual(public["runs"][0]["call"],public["usage_groups"][0]["call"])

    def test_build_does_not_change_database_bytes(self):
        before = hashlib.sha256(self.db.read_bytes()).hexdigest()
        audit.build(self.db)
        self.assertEqual(before,hashlib.sha256(self.db.read_bytes()).hexdigest())

    def test_malformed_diagnostics_are_not_raw_copied(self):
        self.conn.execute("UPDATE computer_use_planner_diagnostics SET response_json=?",(json.dumps({"action":{"kind":{"secret":self.secret},"arguments":{"points":[[0,self.secret]]}}}),))
        self.conn.commit()
        _, public = audit.build(self.db)
        shape = public["planner_diagnostics"][0]["response_structure"]
        self.assertEqual(shape["kind"],"other")
        self.assertFalse(shape["points_numeric"])
        self.assertNotIn(self.secret,json.dumps(public))

    def test_missing_tables_are_explicit(self):
        empty = Path(self.folder.name) / "empty.sqlite3"
        sqlite3.connect(empty).close()
        local, _ = audit.build(empty)
        self.assertEqual(local["summary"]["run_count"],0)
        self.assertIn("computer_use_runs",local["missing_schema_columns"])

    def test_wal_committed_facts_are_visible(self):
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("INSERT INTO chat_usage_events VALUES(?,?,?,?,?,?,?,?,?,?,?)",(3,"private-room","real-session-9",2150,7,3,0,0,"real-turn-3","real-call-17","computer_use_verification"))
        self.conn.commit()
        self.assertTrue(Path(str(self.db)+"-wal").exists())
        _, public = audit.build(self.db)
        self.assertEqual(public["summary"]["cu_usage_totals"]["input_tokens"],107)

    def test_cli_outputs_both_and_rejects_database_overwrite(self):
        output = Path(self.folder.name) / "audit.json"
        subprocess.run([sys.executable,str(SCRIPT),str(self.db),str(output)],check=True,capture_output=True)
        self.assertTrue(output.exists())
        self.assertTrue(output.with_name("audit.public.json").exists())
        before = self.db.read_bytes()
        attempt = subprocess.run([sys.executable,str(SCRIPT),str(self.db),str(self.db)],capture_output=True)
        self.assertNotEqual(attempt.returncode,0)
        self.assertEqual(before,self.db.read_bytes())

    def test_since_filter_and_nonexistent_database(self):
        local, _ = audit.build(self.db,2000)
        self.assertEqual(local["summary"]["run_count"],0)
        self.assertEqual(local["summary"]["usage_record_count"],1)
        nonexistent = Path(self.folder.name) / "absent.sqlite3"
        attempt = subprocess.run([sys.executable,str(SCRIPT),str(nonexistent),str(Path(self.folder.name)/"out.json")],capture_output=True)
        self.assertNotEqual(attempt.returncode,0)
        self.assertFalse(nonexistent.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
