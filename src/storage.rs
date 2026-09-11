//! SQLite owns durable state. Locks protect short database operations, never network waits.
use crate::models::{Job, Settings, now};
use rusqlite::{Connection, params};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub struct Store {
    connection: Mutex<Connection>,
    pub root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let conn = Connection::open(root.join("crawler.sqlite3")).map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS settings (id INTEGER PRIMARY KEY CHECK(id=1), body TEXT NOT NULL); CREATE TABLE IF NOT EXISTS jobs (id TEXT PRIMARY KEY, started TEXT NOT NULL, body TEXT NOT NULL);").map_err(|e| e.to_string())?;
        Ok(Self {
            connection: Mutex::new(conn),
            root: root.into(),
        })
    }
    pub fn settings(&self) -> Result<Settings, String> {
        let db = self.connection.lock().map_err(|e| e.to_string())?;
        let mut stmt = db
            .prepare("SELECT body FROM settings WHERE id=1")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => serde_json::from_str(&row.get::<_, String>(0).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string()),
            None => Ok(Settings::default()),
        }
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings.validate()?;
        self.connection.lock().map_err(|e| e.to_string())?.execute("INSERT INTO settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET body=excluded.body", [serde_json::to_string(settings).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn save(&self, job: &Job) -> Result<(), String> {
        self.connection.lock().map_err(|e| e.to_string())?.execute("INSERT INTO jobs VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET body=excluded.body", params![job.id, job.started_at, serde_json::to_string(job).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<Job>, String> {
        let db = self.connection.lock().map_err(|e| e.to_string())?;
        let mut stmt = db
            .prepare("SELECT body FROM jobs ORDER BY started DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn job(&self, id: &str) -> Result<Option<Job>, String> {
        use rusqlite::OptionalExtension;
        let body: Option<String> = self
            .connection
            .lock()
            .map_err(|e| e.to_string())?
            .query_row("SELECT body FROM jobs WHERE id=?1", [id], |row| row.get(0))
            .optional()
            .map_err(|e| e.to_string())?;
        body.map(|b| serde_json::from_str(&b).map_err(|e| e.to_string()))
            .transpose()
    }
    pub fn recover(&self) -> Result<(), String> {
        for mut job in self.jobs()? {
            if job.status == "running" || job.status == "cancelling" {
                job.status = "interrupted".into();
                job.phase = "伺服器重新啟動，工作已中斷".into();
                job.finished_at = Some(now());
                self.save(&job)?;
            }
        }
        Ok(())
    }
}
