//! Máquina de estados do monitor: deteção de eventos e decisão de shutdown.
//! Pura (sem I/O) para poder ser testada; o `main` chama `tick` a cada 2 s.
use crate::ups::Status;

pub const DEBOUNCE: u32 = 3; // leituras seguidas (~6 s)
pub const TEST_GUARD_S: u64 = 60;
pub const RUNTIME_HOLD_S: u64 = 20;
pub const COMM_GRACE_S: u64 = 30;
pub const COMM_BATT_SHUTDOWN_S: u64 = 30;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Shutdown quando o runtime restante <= N minutos (1..=30).
    KeepRunning(u32),
    /// Shutdown após N minutos em bateria (0..=60).
    Preserve(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PowerFail,
    PowerRestored,
    UnderVoltage,
    OverVoltage,
    Boost,
    Buck,
    RegulationEnd,
    LowBattery,
    CommLost,
    CommRestored,
    SelfTestStart,
    SelfTestPass,
    SelfTestFail,
    ShutdownStart,
    ShutdownCancel,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Event(Kind),
    StartShutdown(&'static str),
    CancelShutdown,
}

pub struct Monitor {
    start: u64,
    batt_n: u32,
    ac_n: u32,
    pub on_batt: bool,
    batt_since: u64,
    guard_until: u64,
    last_test: u8,
    runtimes: Vec<u32>,
    low_since: Option<u64>,
    last_ok: u64,
    pub comm_lost: bool,
    comm_lost_on_batt: bool,
    pub shutdown_active: bool,
    user_cancelled: bool,
    boost: bool,
    buck: bool,
    last_alarm_batt: bool,
}

impl Monitor {
    pub fn new(now: u64) -> Self {
        Monitor {
            start: now, batt_n: 0, ac_n: 0, on_batt: false, batt_since: 0, guard_until: 0, last_test: 6,
            runtimes: vec![], low_since: None, last_ok: now, comm_lost: false, comm_lost_on_batt: false,
            shutdown_active: false, user_cancelled: false, boost: false, buck: false, last_alarm_batt: false,
        }
    }

    /// Chamado quando o utilizador cancela a contagem; fica suspenso até a energia voltar.
    pub fn user_cancel(&mut self) {
        self.shutdown_active = false;
        self.user_cancelled = true;
    }

    pub fn resumed(&mut self, now: u64) { self.start = now; self.last_ok = now; }

    pub fn tick(&mut self, now: u64, st: Option<&Status>, mode: Mode, lim: (u32, u32)) -> Vec<Action> {
        let mut out = vec![];
        let Some(st) = st else {
            // sem comunicação
            if !self.comm_lost && now.saturating_sub(self.last_ok) >= COMM_GRACE_S && now.saturating_sub(self.start) >= COMM_GRACE_S {
                self.comm_lost = true;
                self.comm_lost_on_batt = self.on_batt;
                out.push(Action::Event(Kind::CommLost));
            }
            if self.comm_lost && self.comm_lost_on_batt && !self.shutdown_active && !self.user_cancelled
                && now.saturating_sub(self.last_ok) > COMM_GRACE_S + COMM_BATT_SHUTDOWN_S {
                self.shutdown_active = true;
                out.push(Action::Event(Kind::ShutdownStart));
                out.push(Action::StartShutdown("comunicação com a UPS perdida em bateria"));
            }
            return out;
        };
        self.last_ok = now;
        if self.comm_lost { self.comm_lost = false; out.push(Action::Event(Kind::CommRestored)); }

        // self-test: a UPS passa a bateria durante o teste → não é falha de energia
        if st.test == 5 {
            if self.last_test != 5 { out.push(Action::Event(Kind::SelfTestStart)); }
            self.guard_until = now + TEST_GUARD_S;
        } else if self.last_test == 5 {
            out.push(Action::Event(if st.test == 1 { Kind::SelfTestPass } else { Kind::SelfTestFail }));
        }
        self.last_test = st.test;
        let guarded = now < self.guard_until;

        // boost / buck (só transições)
        if st.boost != self.boost { self.boost = st.boost; out.push(Action::Event(if st.boost { Kind::Boost } else { Kind::RegulationEnd })); }
        if st.buck != self.buck { self.buck = st.buck; out.push(Action::Event(if st.buck { Kind::Buck } else { Kind::RegulationEnd })); }

        // bateria com debounce
        if st.on_battery() && !guarded {
            self.batt_n += 1; self.ac_n = 0;
        } else if !st.on_battery() {
            self.ac_n += 1; self.batt_n = 0;
        }
        if !self.on_batt && self.batt_n >= DEBOUNCE {
            self.on_batt = true;
            self.batt_since = now - 2 * (DEBOUNCE as u64 - 1);
            self.runtimes.clear();
            self.low_since = None;
            let k = if st.in_v != 0 && st.in_v < lim.0 { Kind::UnderVoltage } else if st.in_v > lim.1 { Kind::OverVoltage } else { Kind::PowerFail };
            out.push(Action::Event(k));
        } else if self.on_batt && self.ac_n >= DEBOUNCE {
            self.on_batt = false;
            self.user_cancelled = false;
            self.low_since = None;
            out.push(Action::Event(Kind::PowerRestored));
            if self.shutdown_active {
                self.shutdown_active = false;
                out.push(Action::CancelShutdown);
                out.push(Action::Event(Kind::ShutdownCancel));
            }
        }
        self.last_alarm_batt = self.on_batt;

        if self.on_batt && !self.shutdown_active && !self.user_cancelled {
            if let Some(reason) = self.shutdown_reason(now, st, mode) {
                self.shutdown_active = true;
                out.push(Action::Event(Kind::ShutdownStart));
                out.push(Action::StartShutdown(reason));
            }
        }
        out
    }

    fn shutdown_reason(&mut self, now: u64, st: &Status, mode: Mode) -> Option<&'static str> {
        if st.below_cap_limit || st.time_limit_expired { return Some("a UPS indica bateria no limite"); }
        match mode {
            Mode::Preserve(n) => {
                if now.saturating_sub(self.batt_since) >= n as u64 * 60 { return Some("tempo máximo em bateria atingido"); }
            }
            Mode::KeepRunning(n) => {
                self.runtimes.push(st.runtime_s);
                if self.runtimes.len() > 5 { self.runtimes.remove(0); }
                if self.runtimes.len() >= 3 {
                    let mut s = self.runtimes.clone();
                    s.sort();
                    let med = s[s.len() / 2];
                    if med <= n * 60 {
                        let since = *self.low_since.get_or_insert(now);
                        if now - since >= RUNTIME_HOLD_S { return Some("runtime restante baixo"); }
                    } else {
                        self.low_since = None;
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ac() -> Status { Status { ac_present: true, fully_charged: true, capacity: 100, runtime_s: 1400, in_v: 230, out_v: 230, nominal_w: 900, test: 6, ..Default::default() } }
    fn batt(rt: u32) -> Status { Status { ac_present: false, discharging: true, capacity: 80, runtime_s: rt, in_v: 0, nominal_w: 900, test: 6, ..Default::default() } }
    const LIM: (u32, u32) = (167, 295);

    fn run(m: &mut Monitor, t: &mut u64, st: &Status, n: u32, mode: Mode) -> Vec<Action> {
        let mut all = vec![];
        for _ in 0..n { all.extend(m.tick(*t, Some(st), mode, LIM)); *t += 2; }
        all
    }
    fn has_shutdown(a: &[Action]) -> bool { a.iter().any(|x| matches!(x, Action::StartShutdown(_))) }

    #[test]
    fn flicker_of_two_seconds_is_ignored() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        let mut a = run(&mut m, &mut t, &ac(), 5, Mode::Preserve(0));
        a.extend(run(&mut m, &mut t, &batt(900), 1, Mode::Preserve(0)));
        a.extend(run(&mut m, &mut t, &ac(), 5, Mode::Preserve(0)));
        assert!(a.is_empty(), "{a:?}");
    }

    #[test]
    fn preserve_zero_shuts_down_after_debounce() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(0));
        let a = run(&mut m, &mut t, &batt(900), 4, Mode::Preserve(0));
        assert!(a.contains(&Action::Event(Kind::PowerFail)));
        assert!(has_shutdown(&a));
    }

    #[test]
    fn self_test_does_not_trigger_failure_or_shutdown() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(0));
        let mut testing = batt(900); testing.test = 5;
        let mut a = run(&mut m, &mut t, &testing, 10, Mode::Preserve(0));
        // a UPS ainda em bateria 1 leitura depois do fim do teste (dentro da guarda de 60 s)
        let mut done = batt(900); done.test = 1;
        a.extend(run(&mut m, &mut t, &done, 3, Mode::Preserve(0)));
        a.extend(run(&mut m, &mut t, &ac(), 5, Mode::Preserve(0)));
        assert!(!has_shutdown(&a), "{a:?}");
        assert!(!a.contains(&Action::Event(Kind::PowerFail)));
        assert!(a.contains(&Action::Event(Kind::SelfTestStart)));
        assert!(a.contains(&Action::Event(Kind::SelfTestPass)));
    }

    #[test]
    fn keep_running_ignores_runtime_jump_right_after_failure() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::KeepRunning(5));
        // 1 leitura com runtime absurdamente baixo no início, depois normal
        let mut a = run(&mut m, &mut t, &batt(30), 1, Mode::KeepRunning(5));
        a.extend(run(&mut m, &mut t, &batt(1200), 20, Mode::KeepRunning(5)));
        assert!(!has_shutdown(&a), "{a:?}");
    }

    #[test]
    fn keep_running_shuts_down_when_runtime_low_and_held() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::KeepRunning(5));
        let a = run(&mut m, &mut t, &batt(200), 20, Mode::KeepRunning(5));
        assert!(has_shutdown(&a), "{a:?}");
    }

    #[test]
    fn ac_returns_during_countdown_cancels() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(0));
        let a = run(&mut m, &mut t, &batt(900), 4, Mode::Preserve(0));
        assert!(has_shutdown(&a));
        let b = run(&mut m, &mut t, &ac(), 4, Mode::Preserve(0));
        assert!(b.contains(&Action::CancelShutdown), "{b:?}");
    }

    #[test]
    fn user_cancel_suspends_until_power_returns() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(0));
        run(&mut m, &mut t, &batt(900), 4, Mode::Preserve(0));
        m.user_cancel();
        let a = run(&mut m, &mut t, &batt(900), 10, Mode::Preserve(0));
        assert!(!has_shutdown(&a));
    }

    #[test]
    fn comm_lost_on_ac_only_logs() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(0));
        let mut a = vec![];
        for _ in 0..60 { a.extend(m.tick(t, None, Mode::Preserve(0), LIM)); t += 2; }
        assert!(a.contains(&Action::Event(Kind::CommLost)));
        assert!(!has_shutdown(&a));
    }

    #[test]
    fn comm_lost_on_battery_shuts_down() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(30));
        run(&mut m, &mut t, &batt(900), 4, Mode::Preserve(30));
        let mut a = vec![];
        for _ in 0..60 { a.extend(m.tick(t, None, Mode::Preserve(30), LIM)); t += 2; }
        assert!(has_shutdown(&a), "{a:?}");
    }

    #[test]
    fn battery_limit_flag_shuts_down_immediately() {
        let (mut m, mut t) = (Monitor::new(0), 100u64);
        run(&mut m, &mut t, &ac(), 3, Mode::Preserve(60));
        let mut s = batt(900); s.below_cap_limit = true;
        let a = run(&mut m, &mut t, &s, 4, Mode::Preserve(60));
        assert!(has_shutdown(&a));
    }
}
