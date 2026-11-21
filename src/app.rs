//! Loop de mensagens único: tray + timer de 2 s que lê a UPS, regista eventos e decide o shutdown.
use crate::db::Db;
use crate::hid::Hid;
use crate::monitor::{Action, Kind, Mode, Monitor};
use crate::{shutdown, ups};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_TRAY: u32 = WM_APP + 1;
const ID_TIMER: usize = 1;
const CMD_OPEN: usize = 1;
const CMD_TEST: usize = 2;
const CMD_EXIT: usize = 3;

pub struct App {
    pub hid: Option<Arc<Hid>>,
    mon: Monitor,
    pub db: Db,
    pub st: Option<ups::Status>,
    pub lim: (u32, u32),
    pub mode: Mode,
    pub sounds: bool,
    pub cost_kwh: f64,
    pub co2_kwh: f64,
    t0: Instant,
    wh: f64,
    last_flush: u64,
}

/// Último resultado da thread de leitura da UPS (a leitura USB demora ~150 ms por relatório).
struct Snap { st: Option<ups::Status>, hid: Option<Arc<Hid>>, lim: (u32, u32), at: Instant }
static SNAP: Mutex<Option<Snap>> = Mutex::new(None);
static STOP: AtomicBool = AtomicBool::new(false);

fn poll_loop() {
    let mut hid: Option<Arc<Hid>> = None;
    while !STOP.load(Ordering::Relaxed) {
        if hid.is_none() { hid = Hid::open_ups().map(Arc::new); }
        let st = hid.as_ref().and_then(|h| ups::read_status(h));
        let lim = hid.as_ref().filter(|_| st.is_some()).and_then(|h| ups::limits(h)).map(|l| (l.low.0, l.high.1)).unwrap_or((167, 295));
        if st.is_none() { hid = None; }
        if let Ok(mut g) = SNAP.lock() { *g = Some(Snap { st, hid: hid.clone(), lim, at: Instant::now() }); }
        for _ in 0..10 { if STOP.load(Ordering::Relaxed) { break; } std::thread::sleep(Duration::from_millis(100)); }
    }
}

thread_local! { pub static APP: RefCell<Option<App>> = const { RefCell::new(None) }; }

pub fn unix() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) }

pub fn data_dir() -> String {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let d = format!("{base}\\UPS Panel");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn load_mode(db: &Db) -> Mode {
    let n: u32 = db.get("runtime_min").and_then(|s| s.parse().ok()).unwrap_or(5);
    if db.get("runtime_mode").as_deref() == Some("preserve") { Mode::Preserve(n.min(60)) } else { Mode::KeepRunning(n.clamp(1, 30)) }
}

pub fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::PowerFail => "PowerFail", Kind::PowerRestored => "PowerRestored", Kind::UnderVoltage => "UnderVoltage",
        Kind::OverVoltage => "OverVoltage", Kind::Boost => "Boost", Kind::Buck => "Buck", Kind::RegulationEnd => "RegulationEnd",
        Kind::LowBattery => "LowBattery", Kind::CommLost => "CommLost", Kind::CommRestored => "CommRestored",
        Kind::SelfTestStart => "SelfTestStart", Kind::SelfTestPass => "SelfTestPass", Kind::SelfTestFail => "SelfTestFail",
        Kind::ShutdownStart => "ShutdownStart", Kind::ShutdownCancel => "ShutdownCancel",
    }
}

fn message(k: Kind) -> &'static str {
    match k {
        Kind::PowerFail => "Falha de energia: a UPS passou para bateria",
        Kind::PowerRestored => "Energia restabelecida",
        Kind::UnderVoltage => "Subtensão: a UPS passou para bateria",
        Kind::OverVoltage => "Sobretensão: a UPS passou para bateria",
        Kind::CommLost => "Comunicação com a UPS perdida",
        Kind::CommRestored => "Comunicação com a UPS restabelecida",
        Kind::SelfTestPass => "Auto-teste concluído: passou",
        Kind::SelfTestFail => "Auto-teste concluído: falhou",
        Kind::ShutdownStart => "O computador vai desligar em 30 s",
        Kind::ShutdownCancel => "Desligar cancelado",
        _ => "",
    }
}

impl App {
    fn new() -> Option<App> {
        let db = Db::open(&format!("{}\\ups.db", data_dir())).ok()?;
        let mode = load_mode(&db);
        let sounds = db.get("sounds").map(|s| s != "0").unwrap_or(true);
        let cost_kwh = db.get("cost_kwh").and_then(|s| s.parse().ok()).unwrap_or(0.09);
        let co2_kwh = db.get("co2_kwh").and_then(|s| s.parse().ok()).unwrap_or(0.368);
        Some(App { hid: None, mon: Monitor::new(1000), db, st: None, lim: (167, 295), mode, sounds, cost_kwh, co2_kwh,
                   t0: Instant::now(), wh: 0.0, last_flush: 0 })
    }

    fn now(&self) -> u64 { 1000 + self.t0.elapsed().as_secs() }

    /// Um ciclo de leitura. Devolve o texto do tooltip e uma mensagem opcional para balão.
    fn tick(&mut self) -> (String, Option<(&'static str, bool)>) {
        let snap = SNAP.lock().ok().and_then(|g| g.as_ref().filter(|s| s.at.elapsed() < Duration::from_secs(15)).map(|s| (s.st.clone(), s.hid.clone(), s.lim)));
        match snap {
            Some((st, hid, lim)) => { self.st = st; self.hid = hid; if self.st.is_some() { self.lim = lim; } }
            None => { self.st = None; self.hid = None; }
        }
        let now = self.now();
        let actions = self.mon.tick(now, self.st.as_ref(), self.mode, self.lim);
        let mut balloon = None;
        for a in actions {
            match a {
                Action::Event(k) => {
                    let ts = unix();
                    let _ = self.db.add_event(ts, kind_name(k));
                    match k {
                        Kind::PowerRestored => { let _ = self.db.close_events(ts, &["PowerFail", "UnderVoltage", "OverVoltage"]); }
                        Kind::RegulationEnd => { let _ = self.db.close_events(ts, &["Boost", "Buck"]); }
                        Kind::SelfTestPass => { let _ = self.db.add_test(ts, true); }
                        Kind::SelfTestFail => { let _ = self.db.add_test(ts, false); }
                        _ => {}
                    }
                    let m = message(k);
                    if !m.is_empty() { balloon = Some((m, matches!(k, Kind::PowerFail | Kind::UnderVoltage | Kind::OverVoltage | Kind::CommLost | Kind::ShutdownStart | Kind::SelfTestFail))); }
                    if self.sounds && balloon.is_some() { unsafe { let _ = MessageBeep(MB_ICONEXCLAMATION); } }
                }
                Action::StartShutdown(r) => { if !shutdown::start(r) { let _ = self.db.add_event(unix(), "ShutdownFailed"); } }
                Action::CancelShutdown => { shutdown::cancel(); }
            }
        }
        // energia: acumula Wh e grava a cada 60 s na linha da hora corrente
        if let Some(s) = &self.st { self.wh += s.load_w() as f64 * 2.0 / 3600.0; }
        if now - self.last_flush >= 60 && self.wh > 0.0 {
            let hour = unix() / 3600 * 3600;
            let _ = self.db.add_energy(hour, self.wh, self.cost_kwh, self.co2_kwh);
            self.wh = 0.0;
            self.last_flush = now;
        }
        let tip = match &self.st {
            Some(s) => format!("UPS: {} · {}% · {} min", if s.on_battery() { "Bateria" } else { "AC" }, s.capacity, s.runtime_s / 60),
            None => "UPS: sem comunicação".to_string(),
        };
        (tip, balloon)
    }
}

fn wbuf<const N: usize>(s: &str) -> [u16; N] {
    let mut b = [0u16; N];
    for (i, c) in s.encode_utf16().take(N - 1).enumerate() { b[i] = c; }
    b
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() }
}

fn tray_update(hwnd: HWND, tip: &str, balloon: Option<(&str, bool)>) {
    let mut n = nid(hwnd);
    n.uFlags = NIF_TIP;
    n.szTip = wbuf(tip);
    if let Some((m, warn)) = balloon {
        n.uFlags |= NIF_INFO;
        n.szInfoTitle = wbuf("Lite UPS Panel");
        n.szInfo = wbuf(m);
        n.dwInfoFlags = if warn { NIIF_WARNING } else { NIIF_INFO };
    }
    unsafe { let _ = Shell_NotifyIconW(NIM_MODIFY, &n); }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let mut n = nid(hwnd);
            n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            n.uCallbackMessage = WM_TRAY;
            n.hIcon = LoadIconW(Some(GetModuleHandleW(None).unwrap().into()), PCWSTR(1 as *const u16)).unwrap_or_default();
            n.szTip = wbuf("Lite UPS Panel");
            let _ = Shell_NotifyIconW(NIM_ADD, &n);
            SetTimer(Some(hwnd), ID_TIMER, 2000, None);
            // 1.ª leitura imediata
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(ID_TIMER)), None);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == ID_TIMER => {
            let r = APP.with(|a| a.borrow_mut().as_mut().map(|app| app.tick()));
            if let Some((tip, b)) = r { tray_update(hwnd, &tip, b); }
            LRESULT(0)
        }
        WM_TRAY => {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == WM_RBUTTONUP || ev == WM_LBUTTONDBLCLK {
                if ev == WM_LBUTTONDBLCLK { open_window(hwnd); return LRESULT(0); }
                let menu = CreatePopupMenu().unwrap();
                let _ = AppendMenuW(menu, MF_STRING, CMD_OPEN, w!("Abrir"));
                let _ = AppendMenuW(menu, MF_STRING, CMD_TEST, w!("Auto-teste da UPS"));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT, w!("Sair"));
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                let _ = SetForegroundWindow(hwnd);
                let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None).0 as usize;
                let _ = DestroyMenu(menu);
                match cmd {
                    CMD_OPEN => open_window(hwnd),
                    CMD_TEST => { if let Some(h) = APP.with(|a| a.borrow().as_ref().and_then(|app| app.hid.clone())) { std::thread::spawn(move || { ups::start_self_test(&h); }); } }
                    CMD_EXIT => { let _ = DestroyWindow(hwnd); }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        m if m == WM_APP + 2 => { open_window(hwnd); LRESULT(0) } // 2.ª instância pede para abrir
        WM_DESTROY => {
            STOP.store(true, Ordering::Relaxed);
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid(hwnd));
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Placeholder até a janela de configuração existir: mostra o estado num balão.
fn open_window(hwnd: HWND) {
    crate::ui::open(hwnd);
}

pub fn run(_hidden: bool) {
    unsafe {
        let m = CreateMutexW(None, true, w!("Global\\UPSPanel")).ok();
        if windows::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(other) = FindWindowW(w!("UPSPanelHidden"), PCWSTR::null()) { let _ = PostMessageW(Some(other), WM_APP + 2, WPARAM(0), LPARAM(0)); }
            return;
        }
        let Some(app) = App::new() else { return };
        APP.with(|a| *a.borrow_mut() = Some(app));
        std::thread::spawn(poll_loop);
        let hi: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hi, lpszClassName: w!("UPSPanelHidden"), ..Default::default() };
        RegisterClassW(&wc);
        let _ = CreateWindowExW(WINDOW_EX_STYLE(0), w!("UPSPanelHidden"), w!("Lite UPS Panel"), WS_OVERLAPPED, 0, 0, 0, 0, None, None, Some(hi), None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(m);
    }
}
