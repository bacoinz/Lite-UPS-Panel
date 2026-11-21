//! Persistência SQLite (eventos, energia horária, testes, definições).
use rusqlite::{params, Connection, Result};

pub struct Db { c: Connection }

pub struct EventRow { pub id: i64, pub ts: i64, pub kind: String }

#[derive(Default, Debug, PartialEq)]
pub struct Problem { pub kind: String, pub count: i64, pub secs: i64 }

#[derive(Default, Debug)]
pub struct EnergyReport { pub kwh: f64, pub avg_kwh_day: f64, pub cost: f64, pub co2: f64 }

/// Tipos de evento que têm duração (fecham-se quando a energia/regulação volta).
pub const TIMED: &[&str] = &["PowerFail", "UnderVoltage", "OverVoltage", "Boost", "Buck"];

impl Db {
    pub fn open(path: &str) -> Result<Db> {
        let c = Connection::open(path)?;
        Self::init(c)
    }
    pub fn memory() -> Result<Db> { Self::init(Connection::open_in_memory()?) }

    fn init(c: Connection) -> Result<Db> {
        c.execute_batch(
            "PRAGMA cache_size=-256; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, end_ts INTEGER, kind TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS ev_ts ON events(ts);
             CREATE TABLE IF NOT EXISTS energy(hour INTEGER PRIMARY KEY, wh REAL NOT NULL, cost_kwh REAL NOT NULL, co2_kwh REAL NOT NULL);
             CREATE TABLE IF NOT EXISTS tests(ts INTEGER NOT NULL, passed INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY, v TEXT NOT NULL);",
        )?;
        Ok(Db { c })
    }

    pub fn add_event(&self, ts: i64, kind: &str) -> Result<()> {
        self.c.execute("INSERT INTO events(ts,kind) VALUES(?,?)", params![ts, kind])?;
        Ok(())
    }

    /// Fecha eventos com duração ainda abertos do tipo indicado.
    pub fn close_events(&self, ts: i64, kinds: &[&str]) -> Result<()> {
        for k in kinds {
            self.c.execute("UPDATE events SET end_ts=? WHERE kind=? AND end_ts IS NULL", params![ts, k])?;
        }
        Ok(())
    }

    /// `kind` vazio = todos. `search` filtra por substring do tipo. Devolve (linhas da página, total).
    pub fn events(&self, from: i64, to: i64, kind: &str, search: &str, limit: i64, offset: i64) -> Result<(Vec<EventRow>, i64)> {
        let like = format!("%{search}%");
        let total: i64 = self.c.query_row(
            "SELECT COUNT(*) FROM events WHERE ts>=? AND ts<? AND (?3='' OR kind=?3) AND kind LIKE ?4",
            params![from, to, kind, like], |r| r.get(0))?;
        let mut s = self.c.prepare(
            "SELECT id,ts,kind FROM events WHERE ts>=? AND ts<? AND (?3='' OR kind=?3) AND kind LIKE ?4 ORDER BY ts DESC LIMIT ?5 OFFSET ?6")?;
        let rows = s.query_map(params![from, to, kind, like, limit, offset], |r| Ok(EventRow { id: r.get(0)?, ts: r.get(1)?, kind: r.get(2)? }))?
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, total))
    }

    pub fn delete_events(&self, from: i64, to: i64) -> Result<()> {
        self.c.execute("DELETE FROM events WHERE ts>=? AND ts<?", params![from, to])?;
        Ok(())
    }

    /// Contagem e duração total (s) por tipo desde `since`; eventos abertos contam até `now`.
    pub fn problems(&self, since: i64, now: i64) -> Result<Vec<Problem>> {
        let mut out = vec![];
        for k in TIMED {
            let (n, secs): (i64, i64) = self.c.query_row(
                "SELECT COUNT(*), COALESCE(SUM(COALESCE(end_ts,?2)-ts),0) FROM events WHERE kind=?1 AND ts>=?3",
                params![k, now, since], |r| Ok((r.get(0)?, r.get(1)?)))?;
            out.push(Problem { kind: k.to_string(), count: n, secs });
        }
        Ok(out)
    }

    pub fn add_energy(&self, hour: i64, wh: f64, cost_kwh: f64, co2_kwh: f64) -> Result<()> {
        self.c.execute(
            "INSERT INTO energy(hour,wh,cost_kwh,co2_kwh) VALUES(?1,?2,?3,?4)
             ON CONFLICT(hour) DO UPDATE SET wh=wh+?2",
            params![hour, wh, cost_kwh, co2_kwh])?;
        Ok(())
    }

    pub fn energy_report(&self, from: i64, to: i64) -> Result<EnergyReport> {
        let (wh, cost, co2): (f64, f64, f64) = self.c.query_row(
            "SELECT COALESCE(SUM(wh),0), COALESCE(SUM(wh/1000.0*cost_kwh),0), COALESCE(SUM(wh/1000.0*co2_kwh),0) FROM energy WHERE hour>=? AND hour<?",
            params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let days = ((to - from) as f64 / 86400.0).max(1.0);
        Ok(EnergyReport { kwh: wh / 1000.0, avg_kwh_day: wh / 1000.0 / days, cost, co2 })
    }

    pub fn add_test(&self, ts: i64, passed: bool) -> Result<()> {
        self.c.execute("INSERT INTO tests(ts,passed) VALUES(?,?)", params![ts, passed as i64])?;
        Ok(())
    }
    pub fn last_test(&self) -> Option<(i64, bool)> {
        self.c.query_row("SELECT ts,passed FROM tests ORDER BY ts DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0))).ok()
    }

    pub fn get(&self, k: &str) -> Option<String> {
        self.c.query_row("SELECT v FROM settings WHERE k=?", [k], |r| r.get(0)).ok()
    }
    pub fn set(&self, k: &str, v: &str) -> Result<()> {
        self.c.execute("INSERT INTO settings(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=?2", params![k, v])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_and_problem_summary() {
        let d = Db::memory().unwrap();
        d.add_event(1000, "PowerFail").unwrap();
        d.close_events(1600, &["PowerFail"]).unwrap();
        d.add_event(2000, "PowerFail").unwrap(); // aberto
        d.add_event(2100, "CommLost").unwrap();
        let p = d.problems(0, 2500).unwrap();
        let pf = p.iter().find(|x| x.kind == "PowerFail").unwrap();
        assert_eq!((pf.count, pf.secs), (2, 600 + 500));
        let (rows, total) = d.events(0, 9999, "", "", 10, 0).unwrap();
        assert_eq!((rows.len(), total), (3, 3));
        let (rows, total) = d.events(0, 9999, "CommLost", "", 10, 0).unwrap();
        assert_eq!((rows.len(), total), (1, 1));
        d.delete_events(0, 9999).unwrap();
        assert_eq!(d.events(0, 9999, "", "", 10, 0).unwrap().1, 0);
    }

    #[test]
    fn energy_and_settings() {
        let d = Db::memory().unwrap();
        d.add_energy(0, 500.0, 0.09, 0.368).unwrap();
        d.add_energy(0, 250.0, 0.09, 0.368).unwrap(); // acumula na mesma hora
        d.add_energy(3600, 1250.0, 0.20, 0.368).unwrap();
        let r = d.energy_report(0, 86400).unwrap();
        assert!((r.kwh - 2.0).abs() < 1e-9);
        assert!((r.cost - (0.75 * 0.09 + 1.25 * 0.20)).abs() < 1e-9);
        d.set("k", "a").unwrap(); d.set("k", "b").unwrap();
        assert_eq!(d.get("k").as_deref(), Some("b"));
        d.add_test(5, true).unwrap();
        assert_eq!(d.last_test(), Some((5, true)));
    }
}
