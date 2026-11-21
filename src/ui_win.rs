//! Janela principal: criação (Mica), navegação, fontes e encaminhamento de comandos.
use crate::nav::{self, WM_NAV_SELECT};
use crate::theme::{self, brush, pal, B_CTRL, B_LAYER};
use crate::ui::*;
use crate::ui_pages as pages;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

fn make_font(face: &str, h: i32, wt: i32, dpi: u32) -> HFONT {
    let face = wide(face);
    unsafe {
        CreateFontW(-(h * dpi as i32 / 96), 0, 0, 0, wt, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY, 0, PCWSTR(face.as_ptr()))
    }
}

/// Primeiro tipo de letra da lista realmente instalado (CreateFont substitui em silêncio).
fn pick_face(names: &[&str]) -> String {
    unsafe {
        let dc = GetDC(None);
        let mut found = names[names.len() - 1].to_string();
        for n in names {
            let f = make_font(n, 12, 400, 96);
            let old = SelectObject(dc, f.into());
            let mut buf = [0u16; 64];
            let l = GetTextFaceW(dc, Some(&mut buf)).max(0) as usize;
            SelectObject(dc, old);
            let _ = DeleteObject(f.into());
            if String::from_utf16_lossy(&buf[..l]).trim_end_matches('\0') == *n { found = n.to_string(); break; }
        }
        ReleaseDC(None, dc);
        found
    }
}

fn make_fonts(dpi: u32) {
    let text = pick_face(&["Segoe UI Variable Text", "Segoe UI"]);
    let disp = pick_face(&["Segoe UI Variable Display", "Segoe UI"]);
    let mk = |face: &str, h: i32, wt: i32| make_font(face, h, wt, dpi).0 as isize;
    // 0 corpo · 1 título de secção · 2 valor grande · 3 corpo semi-negrito · 4..7 ícones (18, 12, 20, 8)
    FONTS.with(|f| *f.borrow_mut() = vec![
        mk(&text, 14, 400),
        mk(&disp, 20, 600),
        mk(&disp, 28, 600),
        mk(&text, 14, 600),
        mk("Segoe Fluent Icons", 18, 400),
        mk("Segoe Fluent Icons", 12, 400),
        mk("Segoe Fluent Icons", 20, 400),
        mk("Segoe Fluent Icons", 8, 400),
    ]);
}

fn free_fonts() {
    FONTS.with(|f| for h in f.borrow_mut().drain(..) { unsafe { let _ = DeleteObject(HGDIOBJ(h as *mut _)); } });
}

pub fn open(_owner: HWND) {
    let m = hwnd_of(&MAIN);
    if !m.is_invalid() && unsafe { IsWindow(Some(m)).as_bool() } {
        unsafe { let _ = ShowWindow(m, SW_RESTORE); let _ = SetForegroundWindow(m); }
        return;
    }
    theme::init();
    unsafe {
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_LISTVIEW_CLASSES | ICC_DATE_CLASSES };
        let _ = InitCommonControlsEx(&icc);
        let hi: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let icon = LoadIconW(Some(hi), PCWSTR(1 as *const u16)).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(main_proc), hInstance: hi, lpszClassName: w!("UPSPanelMain"), hIcon: icon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0), ..Default::default() };
        RegisterClassW(&wc);
        let pc = WNDCLASSW { lpfnWndProc: Some(page_proc), hInstance: hi, lpszClassName: w!("UPSPanelPage"),
            hbrBackground: brush(B_LAYER), ..Default::default() };
        RegisterClassW(&pc);
        // DPI do monitor principal (a janela ainda não existe)
        let dpi = GetDpiForWindow(GetDesktopWindow()).max(96);
        DPI.set(dpi);
        make_fonts(dpi);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE;
        let mut r = RECT { left: 0, top: 0, right: d(900), bottom: d(640) };
        let _ = AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE(0));
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), w!("UPSPanelMain"), w!("Lite UPS Panel"), style,
            CW_USEDEFAULT, CW_USEDEFAULT, r.right - r.left, r.bottom - r.top, None, None, Some(hi), None).unwrap_or_default();
        MAIN.set(hwnd.0 as isize);
        // a página (camada) foi construída antes da janela ser mostrada: pintar tudo de novo
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn select_page(i: usize) {
    unsafe {
        let old = page();
        if !old.is_invalid() { let _ = DestroyWindow(old); }
        CUR.set(i);
        EV_PAGE.set(0);
        let main = hwnd_of(&MAIN);
        let hi: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let p = CreateWindowExW(WINDOW_EX_STYLE(0), w!("UPSPanelPage"), w!(""), WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            d(190), 0, d(710), d(640), Some(main), None, Some(hi), None).unwrap_or_default();
        PAGE.set(p.0 as isize);
        theme::panels_clear();
        theme::page_translucent(p);
        if theme::is_mica() {
            // canto superior esquerdo arredondado, como nas apps WinUI sobre Mica
            let rg = CreateRoundRectRgn(0, 0, d(710) + d(16), d(640) + d(16), d(16), d(16));
            SetWindowRgn(p, Some(rg), true);
        }
        pages::build(i, p);
        let nav = GetDlgItem(Some(main), ID_NAV).unwrap_or_default();
        let _ = InvalidateRect(Some(nav), None, false);
    }
}

unsafe extern "system" fn page_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND | WM_PRINTCLIENT => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            theme::paint_page_bg(HDC(wp.0 as *mut _), &rc, d(10));
            LRESULT(1)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(h, &mut ps);
            let _ = EndPaint(h, &ps);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let p = pal();
            let hdc = HDC(wp.0 as *mut _);
            let c = HWND(lp.0 as *mut _);
            let (bgc, bgb) = theme::bg_of(c);
            // rótulos (id 0) em cinzento claro; valores e mensagens em branco
            let id = GetDlgCtrlID(c);
            let heading = SendMessageW(c, WM_GETFONT, None, None).0 == font(1).0 as isize;
            SetTextColor(hdc, COLORREF(if id == 0 && !heading { p.sub } else { p.text }));
            SetBkColor(hdc, COLORREF(bgc));
            LRESULT(bgb.0 as isize)
        }
        0x0133 | 0x0134 => { // CTLCOLOREDIT / CTLCOLORLISTBOX
            let p = pal();
            let hdc = HDC(wp.0 as *mut _);
            SetTextColor(hdc, COLORREF(p.text));
            SetBkColor(hdc, COLORREF(p.ctrl));
            LRESULT(brush(B_CTRL).0 as isize)
        }
        WM_DRAWITEM => {
            let di = &*(lp.0 as *const DRAWITEMSTRUCT);
            theme::draw_item(di, &|i| font(i), &|v| d(v));
            LRESULT(1)
        }
        WM_COMMAND | WM_NOTIFY => {
            if m == WM_NOTIFY && lp.0 != 0 {
                if let Some(r) = theme::lv_customdraw(lp) { return r; }
                let nm = &*(lp.0 as *const NMHDR);
                if nm.code == DTN_DROPDOWN {
                    let mc = HWND(SendMessageW(nm.hwndFrom, DTM_GETMONTHCAL, None, None).0 as *mut _);
                    if !mc.is_invalid() { theme::dark_month_cal(mc); }
                }
            }
            if m == WM_COMMAND && (wp.0 >> 16) & 0xFFFF == 0 && lp.0 != 0 {
                let c = HWND(lp.0 as *mut _);
                if theme::is_toggle(c) { theme::toggle_click(c); }
            }
            SendMessageW(GetParent(h).unwrap_or_default(), m, Some(wp), Some(lp))
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}

unsafe extern "system" fn main_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_CREATE => {
            theme::apply_window(h);
            MAIN.set(h.0 as isize);
            nav::create(h, d(640));
            SetTimer(Some(h), ID_TIMER_UI, 2000, None);
            // constrói a primeira página só depois de a janela estar visível (a camada pinta bem)
            CUR.set(usize::MAX);
            let _ = PostMessageW(Some(h), WM_NAV_SELECT, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let b = if theme::is_mica() { HBRUSH(GetStockObject(BLACK_BRUSH).0) } else { brush(theme::B_SIDE) };
            FillRect(HDC(wp.0 as *mut _), &rc, b);
            LRESULT(1)
        }
        WM_BG => { bg_done(wp.0, lp.0 != 0); LRESULT(0) }
        WM_NAV_SELECT => {
            if wp.0 != CUR.get() { select_page(wp.0); }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wp.0 & 0xFFFF) as i32;
            let code = ((wp.0 >> 16) & 0xFFFF) as u32;
            if id != ID_NAV { pages::command(CUR.get(), id, code); }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == ID_TIMER_UI => {
            pages::refresh(CUR.get());
            let nav = GetDlgItem(Some(h), ID_NAV).unwrap_or_default();
            let _ = InvalidateRect(Some(nav), None, false);
            LRESULT(0)
        }
        WM_CLOSE => { let _ = DestroyWindow(h); LRESULT(0) }
        WM_DESTROY => {
            let _ = KillTimer(Some(h), ID_TIMER_UI);
            free_fonts();
            MAIN.set(0);
            PAGE.set(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
