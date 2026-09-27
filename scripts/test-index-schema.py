"""Disposable SQLite schema checks. Never opens a user FileOrbit database."""
import pathlib
import sqlite3
import tempfile
import unittest

MIGRATION = (pathlib.Path(__file__).resolve().parents[1] / "src-tauri/migrations/0001_index.sql").read_text()


class IndexSchemaTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="fileorbit-schema-")
        self.db = pathlib.Path(self.temp.name) / "fileorbit.db"
        self.conn = sqlite3.connect(self.db)
        self.conn.execute("PRAGMA foreign_keys = ON")

    def tearDown(self):
        self.conn.close()
        self.temp.cleanup()

    def migrate(self):
        try:
            self.conn.executescript("BEGIN IMMEDIATE;\n" + MIGRATION +
                "\nINSERT OR IGNORE INTO schema_migrations(version) VALUES (1);\nCOMMIT;")
        except Exception:
            self.conn.rollback()
            raise

    def test_empty_db_and_repeatable_schema(self):
        self.migrate()
        self.migrate()
        self.assertEqual(self.conn.execute("SELECT count(*) FROM schema_migrations").fetchone()[0], 1)
        self.assertTrue(self.db.exists())
        self.assertEqual(self.conn.execute("PRAGMA foreign_key_check").fetchall(), [])

    def test_test_scan_roundtrip_and_constraints(self):
        self.migrate()
        with self.conn:
            self.conn.execute("INSERT INTO scan_roots(id,path) VALUES('root','/synthetic')")
            self.conn.execute("INSERT INTO scan_runs(id,root_id,started_at,status) VALUES('run','root','2026-01-01','running')")
            self.conn.execute("INSERT INTO folders(id,root_id,path,name,last_seen_run_id) VALUES('folder','root','/synthetic','synthetic','run')")
            self.conn.execute("INSERT INTO files(id,root_id,folder_id,path,name,size_bytes,modified_ns,last_seen_run_id) VALUES('file','root','folder','/synthetic/a.txt','a.txt',3,1,'run')")
        self.assertEqual(self.conn.execute("SELECT name,size_bytes FROM files WHERE id='file'").fetchone(), ("a.txt", 3))
        with self.assertRaises(sqlite3.IntegrityError):
            with self.conn:
                self.conn.execute("INSERT INTO files(id,root_id,folder_id,path,name,size_bytes,modified_ns) VALUES('bad','root','folder','/synthetic/b.txt','b.txt',-1,1)")
        self.assertEqual(self.conn.execute("SELECT count(*) FROM files").fetchone()[0], 1)

    def test_transaction_failure_leaves_no_partial_rows(self):
        self.migrate()
        with self.assertRaises(sqlite3.IntegrityError):
            with self.conn:
                self.conn.execute("INSERT INTO scan_roots(id,path) VALUES('root','/synthetic')")
                self.conn.execute("INSERT INTO folders(id,root_id,path,name) VALUES('bad','missing','/synthetic','synthetic')")
        self.assertEqual(self.conn.execute("SELECT count(*) FROM scan_roots").fetchone()[0], 0)

    def test_three_thousand_metadata_rows_and_reopen(self):
        self.migrate()
        with self.conn:
            self.conn.execute("INSERT INTO scan_roots(id,path) VALUES('root','/synthetic')")
            self.conn.execute("INSERT INTO folders(id,root_id,path,name) VALUES('folder','root','/synthetic','synthetic')")
            self.conn.executemany("INSERT INTO files(id,root_id,folder_id,path,name,size_bytes,modified_ns) VALUES(?,?,?,?,?,?,?)",
                ((str(i),'root','folder',f'/synthetic/file-{i}.txt',f'file-{i}.txt',i,1) for i in range(3000)))
        with self.conn:
            self.conn.executemany("UPDATE files SET size_bytes=? WHERE id=?", ((i+1,str(i)) for i in range(3000)))
        self.assertEqual(self.conn.execute("SELECT count(*) FROM files WHERE path LIKE '/synthetic/%'").fetchone()[0],3000)
        self.assertEqual(self.conn.execute("SELECT size_bytes FROM files WHERE name='file-2999.txt'").fetchone()[0],3000)
        self.conn.close()
        self.conn = sqlite3.connect(self.db)
        self.assertEqual(self.conn.execute("SELECT count(*) FROM files").fetchone()[0],3000)


if __name__ == "__main__":
    unittest.main()
