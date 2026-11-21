//! Janela de configuração em Win32 puro: lista de secções à esquerda, página à direita.
//! A janela é criada ao abrir e destruída ao fechar (liberta memória).
use crate::app::{unix, APP};
use crate::monitor::Mode;
use crate::ups;
use std::cell::{Cell, RefCell};
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

pub(crate) const ID_NAV: i32 = 100;
pub(crate) const ID_TIMER_UI: usize = 77;
pub(crate) const PAGES: [&str; 5] = ["Estado", "Eventos", "Energia", "Configuração", "UPS"];

pub(crate) const WM_BG: u32 = WM_APP + 11;
type Done = Box<dyn FnOnce(bool)>;

thread_local! {
    static DONE: RefCell<std::collections::HashMap<usize, Done>> = RefCell::new(std::collections::HashMap::new());
    static NEXT_BG: Cell<usize> = const { Cell::new(1) };
    pub(crate) static MAIN: Cell<isize> = const { Cell::new(0) };
    pub(crate) static PAGE: Cell<isize> = const { Cell::new(0) };
    pub(crate) static CUR: Cell<usize> = const { Cell::new(0) };
    pub(crate) static DPI: Cell<u32> = const { Cell::new(96) };
    pub(crate) static FONTS: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) }; // normal, bold, big
    pub(crate) static EV_PAGE: Cell<usize> = const { Cell::new(0) };
}

/// Corre `work` noutra thread (escritas USB demoram centenas de ms) e chama `done` na thread da UI.
pub(crate) fn bg(work: impl FnOnce() -> bool + Send + 'static, done: impl FnOnce(bool) + 'static) {
    let id = NEXT_BG.get();
    NEXT_BG.set(id + 1);
    DONE.with(|m| m.borrow_mut().insert(id, Box::new(done)));
    let main = MAIN.get();
    std::thread::spawn(move || {
        let ok = work();
        unsafe { let _ = PostMessageW(Some(HWND(main as *mut _)), WM_BG, WPARAM(id), LPARAM(ok as isize)); }
    });
}
pub(crate) fn bg_done(id: usize, ok: bool) { if let Some(f) = DONE.with(|m| m.borrow_mut().remove(&id)) { f(ok); } }

pub(crate) fn d(x: i32) -> i32 { x * DPI.get() as i32 / 96 }
pub(crate) fn hwnd_of(c: &'static std::thread::LocalKey<Cell<isize>>) -> HWND { HWND(c.get() as *mut _) }
pub(crate) fn page() -> HWND { hwnd_of(&PAGE) }
pub(crate) fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain([0]).collect() }

// ---------- datas ----------
pub(crate) fn tz_offset() -> i64 {
    let mut t = TIME_ZONE_INFORMATION::default();
    let st = unsafe { GetTimeZoneInformation(&mut t) };
    let bias = t.Bias + if st == 2 { t.DaylightBias } else { 0 };
    -(bias as i64) * 60
}
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
pub(crate) fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
pub fn fmt_ts(ts: i64) -> String {
    let l = ts + tz_offset();
    let (y, m, d) = civil_from_days(l.div_euclid(86400));
    let s = l.rem_euclid(86400);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}
pub(crate) fn fmt_dur(secs: i64) -> String {
    if secs <= 0 { return crate::i18n::tr("Nenhum").into(); }
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 { format!("{h} h {m:02} min {s:02} s") } else if m > 0 { format!("{m} min {s:02} s") } else { format!("{s} s") }
}
/// Início do dia local do controlo DateTimePicker (em unix).
pub(crate) fn dtp_day(id: i32) -> i64 {
    let mut st = SYSTEMTIME::default();
    unsafe {
        let h = GetDlgItem(Some(page()), id).unwrap_or_default();
        SendMessageW(h, DTM_GETSYSTEMTIME, None, Some(LPARAM(&mut st as *mut _ as isize)));
    }
    days_from_civil(st.wYear as i64, st.wMonth as i64, st.wDay as i64) * 86400 - tz_offset()
}

// ---------- helpers de controlos ----------
pub(crate) fn font(i: usize) -> HFONT { FONTS.with(|f| HFONT(f.borrow().get(i).copied().unwrap_or(0) as *mut _)) }

#[allow(clippy::too_many_arguments)]
pub(crate) fn mk(parent: HWND, class: PCWSTR, text: &str, x: i32, y: i32, wd: i32, h: i32, id: i32, style: u32, ex: u32, f: usize) -> HWND {
    let t = wide(text);
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let c = CreateWindowExW(WINDOW_EX_STYLE(ex), class, PCWSTR(t.as_ptr()), WINDOW_STYLE(style | 0x5000_0000 /* WS_CHILD|WS_VISIBLE */),
            d(x), d(y), d(wd), d(h), Some(parent), Some(HMENU(id as isize as *mut _)), Some(hi), None).unwrap_or_default();
        SendMessageW(c, WM_SETFONT, Some(WPARAM(font(f).0 as usize)), Some(LPARAM(1)));
        crate::theme::style_control(c);
        c
    }
}
pub(crate) fn label(p: HWND, text: &str, x: i32, y: i32, wd: i32, id: i32) -> HWND { mk(p, w!("STATIC"), text, x, y, wd, 18, id, 0, 0, 0) }
pub(crate) fn heading(p: HWND, text: &str, x: i32, y: i32) {
    mk(p, w!("STATIC"), text, x, y, 420, 28, 0, 0, 0, 1);
}
/// Rótulo centrado verticalmente numa linha de altura `h`.
pub(crate) fn row_label(p: HWND, text: &str, x: i32, y: i32, wd: i32, h: i32, id: i32, f: usize) -> HWND {
    // +1/-1: não tapar a divisória do cartão no topo da linha
    mk(p, w!("STATIC"), text, x, y + 1, wd, h - 1, id, 0x200 /* SS_CENTERIMAGE */, 0, f)
}
/// Cartão de fundo (coordenadas lógicas), com divisórias horizontais nos `div` (y lógicos).
pub(crate) fn panel(x: i32, y: i32, wd: i32, h: i32, div: &[i32]) {
    crate::theme::panel_add(RECT { left: d(x), top: d(y), right: d(x + wd), bottom: d(y + h) }, div.iter().map(|v| d(*v)).collect());
}
pub(crate) fn button(p: HWND, text: &str, x: i32, y: i32, wd: i32, id: i32) -> HWND {
    let c = mk(p, w!("BUTTON"), text, x, y, wd, 30, id, 0x10000 | 0xB, 0, 0);
    if ["Aplicar", "Iniciar auto-teste", "Atualizar", "Procurar"].iter().any(|k| crate::i18n::tr(k) == text) { crate::theme::primary_init(c); }
    c
}
pub(crate) fn check(p: HWND, text: &str, x: i32, y: i32, wd: i32, id: i32) -> HWND {
    let c = mk(p, w!("BUTTON"), text, x, y, wd, 24, id, 0x10000 | 0xB, 0, 0);
    crate::theme::toggle_init(c, false);
    c
}
pub(crate) fn radio(p: HWND, text: &str, x: i32, y: i32, wd: i32, id: i32, group: bool) -> HWND {
    if group { crate::theme::new_group(); }
    let c = mk(p, w!("BUTTON"), text, x, y, wd, 24, id, 0x10000 | 0xB, 0, 0);
    crate::theme::toggle_init(c, true);
    c
}
pub(crate) fn combo(p: HWND, x: i32, y: i32, wd: i32, id: i32, items: &[String], sel: usize) -> HWND {
    let c = mk(p, w!("COMBOBOX"), "", x, y, wd, 220, id, 0x10003 | 0x200000, 0, 0);
    for it in items {
        let t = wide(it);
        unsafe { SendMessageW(c, CB_ADDSTRING, None, Some(LPARAM(t.as_ptr() as isize))); }
    }
    unsafe { SendMessageW(c, CB_SETCURSEL, Some(WPARAM(sel)), None); }
    c
}
pub(crate) fn edit(p: HWND, text: &str, x: i32, y: i32, wd: i32, id: i32) -> HWND { mk(p, w!("EDIT"), text, x, y, wd, 24, id, 0x10080, 0x200, 0) }
pub(crate) fn center(h: HWND) {
    unsafe { SetWindowLongPtrW(h, GWL_STYLE, GetWindowLongPtrW(h, GWL_STYLE) | 0x1 /* SS_CENTER */); }
}
pub(crate) fn cue(h: HWND, text: &str) {
    let t = wide(text);
    unsafe { SendMessageW(h, 0x1501 /* EM_SETCUEBANNER */, Some(WPARAM(1)), Some(LPARAM(t.as_ptr() as isize))); }
}
pub(crate) fn dtp(p: HWND, x: i32, y: i32, id: i32, ts: i64) -> HWND {
    let c = mk(p, w!("SysDateTimePick32"), "", x, y, 110, 24, id, 0x10000, 0, 0);
    let (yy, mm, dd) = civil_from_days((ts + tz_offset()).div_euclid(86400));
    let st = SYSTEMTIME { wYear: yy as u16, wMonth: mm as u16, wDay: dd as u16, ..Default::default() };
    unsafe { SendMessageW(c, DTM_SETSYSTEMTIME, Some(WPARAM(0)), Some(LPARAM(&st as *const _ as isize))); }
    c
}
pub(crate) fn listview(p: HWND, x: i32, y: i32, wd: i32, h: i32, id: i32, cols: &[(&str, i32)]) -> HWND {
    let c = mk(p, w!("SysListView32"), "", x, y, wd, h, id, 0x10001 | 0x4 | 0x8, 0, 0); // REPORT|SINGLESEL|SHOWSELALWAYS
    unsafe {
        SendMessageW(c, LVM_SETEXTENDEDLISTVIEWSTYLE, None, Some(LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER) as isize)));
        // a última coluna ocupa o resto (deixando lugar à barra de deslocamento)
        let used: i32 = cols.iter().take(cols.len() - 1).map(|c| c.1).sum();
        for (i, (t, cw)) in cols.iter().enumerate() {
            let mut tt = wide(t);
            let cw = if i + 1 == cols.len() { wd - used - 22 } else { *cw };
            let col = LVCOLUMNW { mask: LVCF_TEXT | LVCF_WIDTH, cx: d(cw), pszText: PWSTR(tt.as_mut_ptr()), ..Default::default() };
            SendMessageW(c, LVM_INSERTCOLUMNW, Some(WPARAM(i)), Some(LPARAM(&col as *const _ as isize)));
        }
    }
    c
}
pub(crate) fn lv_set(lv: HWND, rows: &[Vec<String>]) {
    unsafe {
        SendMessageW(lv, LVM_DELETEALLITEMS, None, None);
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let mut t = wide(cell);
                let it = LVITEMW { mask: LVIF_TEXT, iItem: r as i32, iSubItem: c as i32, pszText: PWSTR(t.as_mut_ptr()), ..Default::default() };
                if c == 0 { SendMessageW(lv, LVM_INSERTITEMW, None, Some(LPARAM(&it as *const _ as isize))); }
                else { SendMessageW(lv, LVM_SETITEMW, None, Some(LPARAM(&it as *const _ as isize))); }
            }
        }
    }
}
pub(crate) fn item(id: i32) -> HWND { unsafe { GetDlgItem(Some(page()), id).unwrap_or_default() } }
pub(crate) fn set_text(id: i32, s: &str) { let t = wide(s); unsafe { let _ = SetWindowTextW(item(id), PCWSTR(t.as_ptr())); } }
pub(crate) fn get_text(id: i32) -> String {
    let mut b = [0u16; 128];
    let n = unsafe { GetWindowTextW(item(id), &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}
pub(crate) fn enable(id: i32, on: bool) {
    unsafe { let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(item(id), on); }
}
pub(crate) fn checked(id: i32) -> bool { crate::theme::toggle_get(item(id)) }
pub(crate) fn set_check(id: i32, on: bool) { crate::theme::toggle_set(item(id), on); }
pub(crate) fn sel(id: i32) -> usize { unsafe { SendMessageW(item(id), CB_GETCURSEL, None, None).0.max(0) as usize } }
pub(crate) fn set_sel(id: i32, i: usize) { unsafe { SendMessageW(item(id), CB_SETCURSEL, Some(WPARAM(i)), None); } }
pub(crate) fn msg(text: &str) {
    let (t, c) = (wide(text), wide("Lite UPS Panel"));
    unsafe { MessageBoxW(Some(hwnd_of(&MAIN)), PCWSTR(t.as_ptr()), PCWSTR(c.as_ptr()), MB_OK | MB_ICONINFORMATION); }
}
pub(crate) fn with_app<R>(f: impl FnOnce(&mut crate::app::App) -> R) -> Option<R> { APP.with(|a| a.borrow_mut().as_mut().map(f)) }


pub use crate::ui_win::open;
