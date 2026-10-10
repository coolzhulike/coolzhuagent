"""只测试独立 SQLite 夹具；不启动服务器，不读取正式运行库。"""
import hashlib
from contextlib import closing
from pathlib import Path
import runpy
import sqlite3
import tempfile
import unittest

planning = runpy.run_path(str(Path(__file__).with_name('server.py')))['planning']


class PlanningTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='browser-attribution-', dir=Path.cwd() / 'tmp')
        self.db = Path(self.temp.name) / 'fixture.sqlite3'
        with closing(sqlite3.connect(self.db)) as connection, connection:
            connection.executescript('''
                CREATE TABLE chat_room_messages(id TEXT,role TEXT,content TEXT,created_at INTEGER);
                CREATE TABLE runtime_runs(id TEXT,state TEXT,legacy_turn_id TEXT);
                CREATE TABLE runtime_run_events(run_id TEXT,event_type TEXT,payload_json TEXT);
                CREATE TABLE computer_use_runs(call_id TEXT,turn_id TEXT,state TEXT,action_count INTEGER,current_observation_generation INTEGER);
                INSERT INTO chat_room_messages VALUES ('message','user','CASE%_ exact',1);
                INSERT INTO chat_room_messages VALUES ('unrelated','user','CASEXX newer',2);
                INSERT INTO runtime_runs VALUES ('run','running','turn');
                INSERT INTO runtime_run_events VALUES ('run','chat.source_messages','{"message_ids":["message"]}');
                INSERT INTO computer_use_runs VALUES ('call','turn','planning',0,1);
            ''')

    def tearDown(self):
        self.temp.cleanup()

    def test_literal_marker_and_read_only_database(self):
        before = hashlib.sha256(self.db.read_bytes()).hexdigest()
        found = planning(self.db, 'CASE%_')
        self.assertEqual(found['parent_run'], 'run')
        self.assertEqual(found['cu_call_id'], 'call')
        self.assertEqual(found['action_count'], 0)
        self.assertEqual(before, hashlib.sha256(self.db.read_bytes()).hexdigest())
        self.assertIsNone(planning(self.db, 'absent'))

    def test_terminal_or_dispatched_run_does_not_trigger(self):
        with closing(sqlite3.connect(self.db)) as connection, connection:
            connection.execute("UPDATE computer_use_runs SET action_count=1")
        self.assertIsNone(planning(self.db, 'CASE%_'))
        with closing(sqlite3.connect(self.db)) as connection, connection:
            connection.execute("UPDATE computer_use_runs SET action_count=0")
            connection.execute("UPDATE runtime_runs SET state='failed'")
        self.assertIsNone(planning(self.db, 'CASE%_'))

    def test_ambiguous_runtime_binding_is_rejected(self):
        with closing(sqlite3.connect(self.db)) as connection, connection:
            connection.execute("INSERT INTO runtime_runs VALUES ('other','running','other-turn')")
            connection.execute("INSERT INTO runtime_run_events VALUES ('other','chat.source_messages','{\"message_ids\":[\"message\"]}')")
        with self.assertRaisesRegex(ValueError, '多个运行'):
            planning(self.db, 'CASE%_')


if __name__ == '__main__':
    unittest.main()
