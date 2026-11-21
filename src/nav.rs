//! Barra lateral de navegação sobre Mica: desenhada com buffered paint (alfa correto sobre o vidro).
use crate::i18n::tr;
use crate::theme::*;
use crate::ui::*;
use std::cell::Cell;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_NAV_SELECT: u32 = WM_APP + 10;
const ICONS: [&str; 5] = ["\u{E9D9}", "\u{E81C}", "\u{E945}", "\u{E713}", "\u{E7E8}"];

thread_local! {
    static HOT: Cell<i32> = const { Cell::new(-1) };
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}

fn item_rect(i: usize, wd: i32) -> RECT {
    let top = d(14) + i as i32 * (d(40) + d(4));
    RECT { left: d(8), top, right: wd - d(8), bottom: top + d(40) }
}

pub fn create(parent: HWND, h: i32) -> HWND {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let wc = WNDCLASSW { lpfnWndProc: Some(nav_proc), hInstance: hi, lpszClassName: w!("UPSPanelNav"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(), ..Default::default() };
        RegisterClassW(&wc);
        CreateWindowExW(WINDOW_EX_STYLE(0), w!("UPSPanelNav"), w!(""), WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            0, 0, d(190), h, Some(parent), Some(HMENU(ID_NAV as isize as *mut _)), Some(hi), None).unwrap_or_default()
    }
}

fn hit(lp: LPARAM, wd: i32) -> i32 {
    let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
    for i in 0..PAGES.len() {
        let r = item_rect(i, wd);
        if x >= r.left && x < r.right && y >= r.top && y < r.bottom { return i as i32; }
    }
    -1
}

unsafe fn paint(h: HWND) {
    let p = pal();
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(h, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    let (wd, ht) = (rc.right, rc.bottom);
    let mut mem = HDC::default();
    let bp = BeginBufferedPaint(hdc, &rc, BPBF_TOPDOWNDIB, None, &mut mem);
    if bp == 0 { let _ = EndPaint(h, &ps); return; }
    let _ = BufferedPaintClear(bp, None);
    let mica = is_mica();
    if !mica {
        FillRect(mem, &rc, brush(B_SIDE));
        let _ = BufferedPaintSetAlpha(bp, None, 255);
    }
    let cur = CUR.get();
    let hot = HOT.get();
    // máscaras (magenta = destaque suave, ciano = barra de acento)
    let mask = |c: u32, r: &RECT, rad: i32| {
        let b = CreateSolidBrush(COLORREF(c));
        let np = GetStockObject(NULL_PEN);
        let (ob, op) = (SelectObject(mem, b.into()), SelectObject(mem, np));
        let _ = RoundRect(mem, r.left, r.top, r.right, r.bottom, rad, rad);
        SelectObject(mem, ob);
        SelectObject(mem, op);
        let _ = DeleteObject(b.into());
    };
    const M_PILL: u32 = 0x00FF00FF;
    const M_BAR: u32 = 0x00FFFF00; // COLORREF: R=0,G=255,B=255
    const M_HOT: u32 = 0x0000FFFF; // R=255,G=255,B=0
    for i in 0..PAGES.len() {
        let r = item_rect(i, wd);
        if i == cur { mask(M_PILL, &r, d(10)); } else if i as i32 == hot { mask(M_HOT, &r, d(10)); }
    }
    let sel = item_rect(cur.min(PAGES.len() - 1), wd);
    let bar = RECT { left: sel.left, top: sel.top + d(11), right: sel.left + d(3), bottom: sel.bottom - d(11) };
    mask(M_BAR, &bar, d(3));
    // passagem de alfa
    let mut bits: *mut RGBQUAD = std::ptr::null_mut();
    let mut row = 0i32;
    if GetBufferedPaintBits(bp, &mut bits, &mut row).is_ok() && !bits.is_null() {
        let px = bits as *mut u32;
        let (ov, base) = if p.dark { (255u32, 0u32) } else { (0, 0) };
        let bgc = p.side;
        for y in 0..ht {
            for x in 0..wd {
                let q = px.add((y * row + x) as usize);
                let v = *q & 0x00FF_FFFF;
                // memória BGRA: 0x00BBGGRR do COLORREF corresponde a R=low byte; DIB é B,G,R -> u32 = R<<16|G<<8|B
                let (r, g, b) = ((v >> 16) & 255, (v >> 8) & 255, v & 255);
                let is_pill = r == 255 && g == 0 && b == 255;
                let is_hot = r == 255 && g == 255 && b == 0;
                let is_bar = r == 0 && g == 255 && b == 255;
                if is_pill || is_hot {
                    // branco translúcido (pré-multiplicado): seleção mais forte que hover
                    let a = if is_pill { 0x16u32 } else { 0x0Du32 };
                    if mica { *q = a << 24 | (ov * a / 255) * 0x0001_0101; }
                    else { let c = mix(bgc, rgb(255, 255, 255), if is_pill { 9 } else { 5 }); let (cr, cg, cb) = split(c); *q = 0xFF00_0000 | cr << 16 | cg << 8 | cb; }
                } else if is_bar {
                    let (ar, ag, ab) = split(p.accent);
                    *q = 0xFF00_0000 | ar << 16 | ag << 8 | ab;
                }
                let _ = base;
            }
        }
    }
    // texto composto (alfa correto)
    let th = OpenThemeData(Some(h), w!("TEXTSTYLE"));
    let draw = |s: &str, r: &mut RECT, color: u32, font: HFONT, fl: DRAW_TEXT_FORMAT| {
        let old = SelectObject(mem, font.into());
        let opts = DTTOPTS { dwSize: std::mem::size_of::<DTTOPTS>() as u32, dwFlags: DTT_COMPOSITED | DTT_TEXTCOLOR, crText: COLORREF(color), ..Default::default() };
        let v: Vec<u16> = s.encode_utf16().collect();
        let _ = DrawThemeTextEx(th, mem, 0, 0, &v, fl | DT_NOPREFIX | DT_SINGLELINE, r, Some(&opts));
        SelectObject(mem, old);
    };
    if th.0 != 0 {
        for (i, name) in PAGES.iter().enumerate() {
            let r = item_rect(i, wd);
            let col = if i == cur { p.text } else { mix(p.text, p.sub, 25) };
            let mut ir = RECT { left: r.left + d(14), top: r.top, right: r.left + d(40), bottom: r.bottom };
            draw(ICONS[i], &mut ir, if i == cur { p.accent } else { col }, font(4), DT_LEFT | DT_VCENTER);
            let mut tr = RECT { left: r.left + d(46), top: r.top, right: r.right - d(6), bottom: r.bottom };
            draw(crate::i18n::tr(name), &mut tr, col, font(0), DT_LEFT | DT_VCENTER);
        }
        // estado da UPS no rodapé
        let st = with_app(|a| a.st.clone()).flatten();
        let (l1, l2, dot) = match &st {
            Some(s) => (
                (if s.on_battery() { tr("Em bateria") } else { tr("Rede elétrica") }).to_string(),
                format!("{} %  ·  {} min", s.capacity, s.runtime_s / 60),
                if s.on_battery() { rgb(252, 180, 0) } else { rgb(16, 190, 80) },
            ),
            None => (tr("Sem comunicação").to_string(), tr("UPS não encontrada").to_string(), rgb(220, 60, 50)),
        };
        let y = ht - d(64);
        let mut dr = RECT { left: d(20), top: y, right: d(40), bottom: y + d(22) };
        draw("\u{25CF}", &mut dr, dot, font(0), DT_LEFT | DT_VCENTER);
        let mut a = RECT { left: d(38), top: y, right: wd - d(8), bottom: y + d(22) };
        draw(&l1, &mut a, p.text, font(3), DT_LEFT | DT_VCENTER);
        let mut b = RECT { left: d(38), top: y + d(22), right: wd - d(8), bottom: y + d(44) };
        draw(&l2, &mut b, p.sub, font(0), DT_LEFT | DT_VCENTER);
        let _ = CloseThemeData(th);
    }
    let _ = EndBufferedPaint(bp, true);
    let _ = EndPaint(h, &ps);
}

unsafe extern "system" fn nav_proc(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => { paint(h); LRESULT(0) }
        WM_GETDLGCODE => LRESULT(1), // DLGC_WANTARROWS
        WM_MOUSEMOVE => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let i = hit(lp, rc.right);
            if i != HOT.get() { HOT.set(i); let _ = InvalidateRect(Some(h), None, false); }
            if !TRACKING.get() {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                TRACKING.set(true);
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => { TRACKING.set(false); HOT.set(-1); let _ = InvalidateRect(Some(h), None, false); LRESULT(0) }
        WM_LBUTTONDOWN => {
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            let i = hit(lp, rc.right);
            let _ = SetFocus(Some(h));
            if i >= 0 { SendMessageW(GetParent(h).unwrap_or_default(), WM_NAV_SELECT, Some(WPARAM(i as usize)), None); }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let c = CUR.get() as i32;
            let n = PAGES.len() as i32;
            let t = match VIRTUAL_KEY(wp.0 as u16) { VK_DOWN => (c + 1).min(n - 1), VK_UP => (c - 1).max(0), _ => c };
            if t != c { SendMessageW(GetParent(h).unwrap_or_default(), WM_NAV_SELECT, Some(WPARAM(t as usize)), None); }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, wp, lp),
    }
}
