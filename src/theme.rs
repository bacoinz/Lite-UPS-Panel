//! Aparência Windows 11 escura: acrílico, cartões, cor de destaque e desenho dos controlos.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use windows::core::{w, PCSTR, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 { r as u32 | (g as u32) << 8 | (b as u32) << 16 }
pub fn split(c: u32) -> (u32, u32, u32) { (c & 255, c >> 8 & 255, c >> 16 & 255) }
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let (ar, ag, ab) = split(a);
    let (br, bg, bb) = split(b);
    let f = |x: u32, y: u32| (x * (100 - t) + y * t) / 100;
    f(ar, br) | f(ag, bg) << 8 | f(ab, bb) << 16
}

#[derive(Clone, Copy)]
pub struct Pal { pub dark: bool, pub layer: u32, pub card: u32, pub border: u32, pub text: u32, pub sub: u32,
    pub ctrl: u32, pub press: u32, pub accent: u32, pub on_accent: u32, pub side: u32 }

/// Cartão de fundo (coordenadas da página) com linhas divisórias opcionais (y absolutos).
struct Panel { rc: RECT, div: Vec<i32> }

thread_local! {
    static PAL: Cell<Option<Pal>> = const { Cell::new(None) };
    static MICA: Cell<bool> = const { Cell::new(false) };
    static BR: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    static GROUP: Cell<u32> = const { Cell::new(0) };
    static CARDS: RefCell<HashMap<i32, (String, String)>> = RefCell::new(HashMap::new());
    static PANELS: RefCell<Vec<Panel>> = const { RefCell::new(Vec::new()) };
}

fn reg_dword(sub: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut v = 0u32;
    let mut sz = 4u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, sub, name, RRF_RT_REG_DWORD, None, Some(&mut v as *mut _ as *mut c_void), Some(&mut sz)) };
    if r.0 == 0 { Some(v) } else { None }
}

pub fn pal() -> Pal { PAL.get().unwrap_or_else(|| { init(); PAL.get().unwrap() }) }
pub fn is_mica() -> bool { MICA.get() }
pub fn brush(i: usize) -> HBRUSH { BR.with(|b| HBRUSH(b.borrow().get(i).copied().unwrap_or(0) as *mut _)) }
pub const B_LAYER: usize = 0;
pub const B_CTRL: usize = 1;
pub const B_SIDE: usize = 2;
pub const B_CARD: usize = 3;

pub fn init() {
    if PAL.get().is_some() { return; }
    // sempre escuro; só a cor de destaque vem do sistema
    let mut accent = reg_dword(w!("Software\\Microsoft\\Windows\\DWM"), w!("AccentColor")).map(|v| v & 0x00FF_FFFF).unwrap_or(rgb(0, 120, 212));
    let (sr, sg, sb) = split(accent);
    if sr.max(sg).max(sb) - sr.min(sg).min(sb) < 40 { accent = rgb(0, 120, 212); }
    // no escuro o destaque é uma versão mais clara (como o Windows faz: AccentLight2)
    let lum = |c: u32| { let (r, g, b) = split(c); (r * 299 + g * 587 + b * 114) / 1000 };
    while lum(accent) < 120 { accent = mix(accent, rgb(255, 255, 255), 18); }
    let on_accent = if lum(accent) > 150 { rgb(0, 0, 0) } else { rgb(255, 255, 255) };
    let p = Pal { dark: true, layer: rgb(28, 28, 30), card: rgb(40, 40, 43), border: rgb(54, 54, 58), text: rgb(255, 255, 255),
        sub: rgb(190, 190, 194), ctrl: rgb(50, 50, 54), press: rgb(40, 40, 43), accent, on_accent, side: rgb(32, 32, 34) };
    PAL.set(Some(p));
    BR.with(|b| *b.borrow_mut() = [p.layer, p.ctrl, p.side, p.card].iter().map(|c| unsafe { CreateSolidBrush(COLORREF(*c)).0 as isize }).collect());
    // menus (tray) escuros: uxtheme #135 SetPreferredAppMode(ForceDark), #136 FlushMenuThemes
    unsafe {
        if let Ok(m) = LoadLibraryW(w!("uxtheme.dll")) {
            if let Some(f) = GetProcAddress(m, PCSTR(135 as *const u8)) {
                let f: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                f(2);
            }
            if let Some(g) = GetProcAddress(m, PCSTR(136 as *const u8)) {
                let g: extern "system" fn() = std::mem::transmute(g);
                g();
            }
        }
    }
}

/// Acrílico escuro em toda a janela; a página de conteúdo fica por cima, quase opaca.
pub fn apply_window(h: HWND) {
    unsafe {
        let d = 1i32;
        let _ = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(20), &d as *const _ as *const c_void, 4);
        // DWMSBT_TRANSIENTWINDOW = Acrílico (3); se não existir, Mica (2)
        let mut bd = 3i32;
        let mut ok = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(38), &bd as *const _ as *const c_void, 4).is_ok();
        if !ok { bd = 2; ok = DwmSetWindowAttribute(h, DWMWINDOWATTRIBUTE(38), &bd as *const _ as *const c_void, 4).is_ok(); }
        if ok {
            let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
            let _ = DwmExtendFrameIntoClientArea(h, &m);
        }
        MICA.set(ok);
    }
}

/// Página de conteúdo com um toque de transparência sobre o acrílico.
pub fn page_translucent(p: HWND) {
    if !is_mica() { return; }
    unsafe {
        SetWindowLongPtrW(p, GWL_EXSTYLE, GetWindowLongPtrW(p, GWL_EXSTYLE) | WS_EX_LAYERED.0 as isize);
        let _ = SetLayeredWindowAttributes(p, COLORREF(0), 240, LWA_ALPHA);
    }
}

// ---------- cartões de fundo da página ----------
pub fn panels_clear() { PANELS.with(|p| p.borrow_mut().clear()); }
pub fn panel_add(rc: RECT, div: Vec<i32>) { PANELS.with(|p| p.borrow_mut().push(Panel { rc, div })); }

/// Fundo da página: camada + cartões (usado também pelo DrawThemeParentBackground dos controlos).
pub unsafe fn paint_page_bg(hdc: HDC, client: &RECT, radius: i32) {
    let p = pal();
    FillRect(hdc, client, brush(B_LAYER));
    PANELS.with(|ps| {
        for pn in ps.borrow().iter() {
            rr(hdc, &pn.rc, radius, p.card, p.border);
            if !pn.div.is_empty() {
                let b = CreateSolidBrush(COLORREF(mix(p.card, p.border, 70)));
                for y in &pn.div { FillRect(hdc, &RECT { left: pn.rc.left + 1, top: *y, right: pn.rc.right - 1, bottom: *y + 1 }, b); }
                let _ = DeleteObject(b.into());
            }
        }
    });
}

/// Cor/pincel de fundo atrás de um controlo (cartão se estiver dentro de um).
pub fn bg_of(c: HWND) -> (u32, HBRUSH) {
    unsafe {
        let mut r = RECT::default();
        let _ = GetWindowRect(c, &mut r);
        let parent = GetParent(c).unwrap_or_default();
        let mut pt = POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 };
        let _ = ScreenToClient(parent, &mut pt);
        let inside = PANELS.with(|ps| ps.borrow().iter().any(|pn| pt.x > pn.rc.left && pt.x < pn.rc.right && pt.y > pn.rc.top && pt.y < pn.rc.bottom));
        let p = pal();
        if inside { (p.card, brush(B_CARD)) } else { (p.layer, brush(B_LAYER)) }
    }
}

// ---------- lista: cabeçalho desenhado por nós ----------
unsafe extern "system" fn hdr_subclass(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, _id: usize, font: usize) -> LRESULT {
    match m {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let p = pal();
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(h, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(h, &mut rc);
            FillRect(hdc, &rc, brush(B_CARD));
            let ln = CreateSolidBrush(COLORREF(p.border));
            FillRect(hdc, &RECT { left: rc.left, top: rc.bottom - 1, right: rc.right, bottom: rc.bottom }, ln);
            let n = SendMessageW(h, HDM_GETITEMCOUNT, None, None).0.max(0);
            for i in 0..n {
                let mut ir = RECT::default();
                SendMessageW(h, HDM_GETITEMRECT, Some(WPARAM(i as usize)), Some(LPARAM(&mut ir as *mut _ as isize)));
                let mut t = [0u16; 128];
                let mut it = HDITEMW { mask: HDI_TEXT, pszText: PWSTR(t.as_mut_ptr()), cchTextMax: 127, ..Default::default() };
                SendMessageW(h, HDM_GETITEMW, Some(WPARAM(i as usize)), Some(LPARAM(&mut it as *mut _ as isize)));
                let len = t.iter().position(|&c| c == 0).unwrap_or(0);
                let mut r = RECT { left: ir.left + 10, top: ir.top, right: ir.right - 6, bottom: ir.bottom - 1 };
                text(hdc, &String::from_utf16_lossy(&t[..len]), &mut r, p.sub, HFONT(font as *mut _), DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
                if i + 1 < n {
                    FillRect(hdc, &RECT { left: ir.right - 1, top: ir.top + 8, right: ir.right, bottom: ir.bottom - 8 }, ln);
                }
            }
            let _ = DeleteObject(ln.into());
            let _ = EndPaint(h, &ps);
            return LRESULT(0);
        }
        WM_NCDESTROY => { let _ = RemoveWindowSubclass(h, Some(hdr_subclass), 2); }
        _ => {}
    }
    DefSubclassProc(h, m, wp, lp)
}

/// Lista: o controlo desenha linhas verticais entre colunas; apagamo-las no fim do desenho
/// copiando a coluna de píxeis ao lado (mantém o realce da linha selecionada).
pub unsafe fn lv_customdraw(lp: LPARAM) -> Option<LRESULT> {
    let nm = &*(lp.0 as *const NMCUSTOMDRAW);
    if nm.hdr.code != NM_CUSTOMDRAW { return None; }
    let mut cls = [0u16; 32];
    let n = GetClassNameW(nm.hdr.hwndFrom, &mut cls) as usize;
    if String::from_utf16_lossy(&cls[..n]) != "SysListView32" { return None; }
    match nm.dwDrawStage {
        CDDS_PREPAINT => Some(LRESULT(CDRF_NOTIFYPOSTPAINT as isize)),
        CDDS_POSTPAINT => {
            let lv = nm.hdr.hwndFrom;
            let mut rc = RECT::default();
            let _ = GetClientRect(lv, &mut rc);
            let mut x = 0;
            let mut i = 0;
            loop {
                let w = SendMessageW(lv, LVM_GETCOLUMNWIDTH, Some(WPARAM(i)), None).0 as i32;
                if w <= 0 { break; }
                x += w;
                let _ = BitBlt(nm.hdc, x - 2, rc.top, 1, rc.bottom - rc.top, Some(nm.hdc), x - 3, rc.top, SRCCOPY);
                i += 1;
            }
            Some(LRESULT(0))
        }
        _ => None,
    }
}

// ---------- data/calendário: o controlo pinta claro; remapeamos para escuro ----------
unsafe fn remap_paint(h: HWND, hdc: HDC, bg: u32) {
    let p = pal();
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    let (w, ht) = (rc.right, rc.bottom);
    if w <= 0 || ht <= 0 { return; }
    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -ht,
        biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() }, ..Default::default() };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
    let mem = CreateCompatibleDC(Some(hdc));
    let old = SelectObject(mem, bmp.into());
    // o controlo já pintou na janela (claro); lemos o resultado
    let _ = BitBlt(mem, 0, 0, w, ht, Some(hdc), 0, 0, SRCCOPY);
    let _ = GdiFlush();
    let px = std::slice::from_raw_parts_mut(bits as *mut u32, (w * ht) as usize);
    let (br, bgc, bb) = split(bg);
    let (fr, fg, fb) = split(p.text);
    let (ar, ag, ab) = split(p.accent);
    for q in px.iter_mut() {
        let (r, g, b) = ((*q >> 16) & 255, (*q >> 8) & 255, *q & 255);
        let sat = r.max(g).max(b) - r.min(g).min(b);
        if sat > 90 {
            // realce do sistema (azul) -> cor de destaque com a mesma intensidade
            let l = 255 - (r * 299 + g * 587 + b * 114) / 1000;
            let k = (l * 2).min(255);
            *q = (ar * k / 255) << 16 | (ag * k / 255) << 8 | (ab * k / 255);
            continue;
        }
        let t = 255 - (r * 299 + g * 587 + b * 114) / 1000;
        let f = |a: u32, z: u32| (a * (255 - t) + z * t) / 255;
        *q = f(br, fr) << 16 | f(bgc, fg) << 8 | f(bb, fb);
    }
    let _ = BitBlt(hdc, 0, 0, w, ht, Some(mem), 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteDC(mem);
    let _ = DeleteObject(bmp.into());
}

unsafe extern "system" fn dark_subclass(h: HWND, m: u32, wp: WPARAM, lp: LPARAM, id: usize, _d: usize) -> LRESULT {
    match m {
        WM_PAINT => {
            let r = DefSubclassProc(h, m, wp, lp);
            let hdc = GetDC(Some(h));
            remap_paint(h, hdc, if id == 3 { pal().ctrl } else { pal().card });
            ReleaseDC(Some(h), hdc);
            return r;
        }
        WM_NCPAINT if id == 4 => {
            // moldura do calendário: cinzenta em vez de branca
            let hdc = GetWindowDC(Some(h));
            let (mut wr, mut cr) = (RECT::default(), RECT::default());
            let _ = GetWindowRect(h, &mut wr);
            let _ = GetClientRect(h, &mut cr);
            let mut o = POINT::default();
            let _ = ClientToScreen(h, &mut o);
            let (cx, cy) = (o.x - wr.left, o.y - wr.top);
            ExcludeClipRect(hdc, cx, cy, cx + cr.right, cy + cr.bottom);
            let b = CreateSolidBrush(COLORREF(pal().border));
            FillRect(hdc, &RECT { left: 0, top: 0, right: wr.right - wr.left, bottom: wr.bottom - wr.top }, b);
            let _ = DeleteObject(b.into());
            ReleaseDC(Some(h), hdc);
            return LRESULT(0);
        }
        WM_NCDESTROY => { let _ = RemoveWindowSubclass(h, Some(dark_subclass), id); }
        _ => {}
    }
    DefSubclassProc(h, m, wp, lp)
}

/// Calendário que abre por baixo do seletor de data.
pub fn dark_month_cal(mc: HWND) {
    unsafe {
        let _ = SetWindowSubclass(mc, Some(dark_subclass), 4, 0);
        let _ = RedrawWindow(Some(mc), None, None, RDW_INVALIDATE | RDW_FRAME);
    }
}

/// Tema escuro nos controlos comuns.
pub fn style_control(c: HWND) {
    let mut cls = [0u16; 64];
    let n = unsafe { GetClassNameW(c, &mut cls) } as usize;
    let name = String::from_utf16_lossy(&cls[..n]);
    let p = pal();
    unsafe {
        match name.as_str() {
            "SysListView32" => {
                let _ = SetWindowTheme(c, w!("DarkMode_Explorer"), PCWSTR::null());
                SendMessageW(c, LVM_SETBKCOLOR, None, Some(LPARAM(p.card as isize)));
                SendMessageW(c, LVM_SETTEXTBKCOLOR, None, Some(LPARAM(p.card as isize)));
                SendMessageW(c, LVM_SETTEXTCOLOR, None, Some(LPARAM(p.text as isize)));
                let hd = HWND(SendMessageW(c, LVM_GETHEADER, None, None).0 as *mut _);
                if !hd.is_invalid() {
                    let f = SendMessageW(c, WM_GETFONT, None, None).0 as usize;
                    let _ = SetWindowSubclass(hd, Some(hdr_subclass), 2, f);
                }
            }
            "SysDateTimePick32" => { let _ = SetWindowSubclass(c, Some(dark_subclass), 3, 0); }
            "ComboBox" | "Edit" => { let _ = SetWindowTheme(c, w!("DarkMode_CFD"), PCWSTR::null()); }
            _ => {}
        }
    }
}

// ---------- estado dos botões personalizados (userdata) ----------
const U_CHECKED: isize = 1;
const U_RADIO: isize = 2;
const U_TOGGLE: isize = 4;
const U_PRIMARY: isize = 8;
fn ud(h: HWND) -> isize { unsafe { GetWindowLongPtrW(h, GWLP_USERDATA) } }
fn set_ud(h: HWND, v: isize) { unsafe { SetWindowLongPtrW(h, GWLP_USERDATA, v); } }
pub fn new_group() { GROUP.set(GROUP.get() + 1); }
pub fn toggle_init(h: HWND, radio: bool) { set_ud(h, U_TOGGLE | if radio { U_RADIO } else { 0 } | (GROUP.get() as isize) << 8); }
pub fn primary_init(h: HWND) { set_ud(h, U_PRIMARY); }
pub fn is_toggle(h: HWND) -> bool { ud(h) & U_TOGGLE != 0 }
pub fn toggle_get(h: HWND) -> bool { ud(h) & U_CHECKED != 0 }
pub fn toggle_set(h: HWND, on: bool) {
    set_ud(h, (ud(h) & !U_CHECKED) | if on { U_CHECKED } else { 0 });
    unsafe { let _ = InvalidateRect(Some(h), None, false); }
}
pub fn toggle_click(h: HWND) {
    let v = ud(h);
    if v & U_RADIO == 0 { toggle_set(h, v & U_CHECKED == 0); return; }
    let grp = v >> 8;
    unsafe {
        let parent = GetParent(h).unwrap_or_default();
        let mut c = GetWindow(parent, GW_CHILD).unwrap_or_default();
        while !c.is_invalid() {
            let u = ud(c);
            if u & U_TOGGLE != 0 && u & U_RADIO != 0 && u >> 8 == grp { toggle_set(c, c == h); }
            c = GetWindow(c, GW_HWNDNEXT).unwrap_or_default();
        }
    }
}
pub fn card_init(id: i32, caption: &str, glyph: &str) { CARDS.with(|c| { c.borrow_mut().insert(id, (caption.into(), glyph.into())); }); }

// ---------- desenho ----------
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().collect() }
pub unsafe fn rr(hdc: HDC, rc: &RECT, r: i32, fill: u32, line: u32) {
    let b = CreateSolidBrush(COLORREF(fill));
    let p = CreatePen(PS_SOLID, 1, COLORREF(line));
    let (ob, op) = (SelectObject(hdc, b.into()), SelectObject(hdc, p.into()));
    let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, r, r);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(b.into());
    let _ = DeleteObject(p.into());
}
unsafe fn text(hdc: HDC, s: &str, rc: &mut RECT, color: u32, font: HFONT, flags: DRAW_TEXT_FORMAT) {
    let old = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(color));
    let mut v = wide(s);
    DrawTextW(hdc, &mut v, rc, flags | DT_NOPREFIX);
    SelectObject(hdc, old);
}
unsafe fn outline(hdc: HDC, f: &RECT, r: i32, color: u32, width: i32) {
    let pen = CreatePen(PS_SOLID, width, COLORREF(color));
    let (ob, op) = (SelectObject(hdc, GetStockObject(NULL_BRUSH)), SelectObject(hdc, pen.into()));
    let _ = RoundRect(hdc, f.left, f.top, f.right, f.bottom, r, r);
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(pen.into());
}

/// fonts: 0 corpo · 2 valor grande · 4 ícones 18 · 5 ícones 12 · 6 ícones 20 · 7 ícones 8
pub fn draw_item(di: &DRAWITEMSTRUCT, fonts: &dyn Fn(usize) -> HFONT, d: &dyn Fn(i32) -> i32) {
    let p = pal();
    let (hdc, mut rc, h) = (di.hDC, di.rcItem, di.hwndItem);
    let id = di.CtlID as i32;
    let (bgc, bgb) = bg_of(h);
    unsafe {
        let mut t = [0u16; 256];
        let n = GetWindowTextW(h, &mut t) as usize;
        let label = String::from_utf16_lossy(&t[..n]);
        FillRect(hdc, &rc, bgb);
        if di.CtlType == ODT_STATIC {
            if let Some((cap, glyph)) = CARDS.with(|c| c.borrow().get(&id).cloned()) {
                rr(hdc, &rc, d(10), p.card, p.border);
                let mut a = RECT { left: rc.left + d(16), top: rc.top + d(14), right: rc.right - d(40), bottom: rc.top + d(36) };
                text(hdc, &cap, &mut a, p.sub, fonts(0), DT_LEFT | DT_SINGLELINE);
                let mut g = RECT { left: rc.right - d(44), top: rc.top + d(12), right: rc.right - d(14), bottom: rc.top + d(38) };
                text(hdc, &glyph, &mut g, p.accent, fonts(6), DT_RIGHT | DT_SINGLELINE);
                let mut v = RECT { left: rc.left + d(16), top: rc.top + d(44), right: rc.right - d(12), bottom: rc.bottom - d(10) };
                text(hdc, &label, &mut v, p.text, fonts(2), DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
            }
            return;
        }
        let u = ud(h);
        let pressed = di.itemState.0 & ODS_SELECTED.0 != 0;
        let focus = di.itemState.0 & ODS_FOCUS.0 != 0 && di.itemState.0 & ODS_NOFOCUSRECT.0 == 0;
        let off = di.itemState.0 & ODS_DISABLED.0 != 0;
        let txt = if off { mix(p.text, bgc, 55) } else { p.text };
        if u & U_TOGGLE != 0 {
            let on = u & U_CHECKED != 0;
            let sz = d(20);
            let top = (rc.top + rc.bottom - sz) / 2;
            let bx = RECT { left: rc.left + d(1), top, right: rc.left + d(1) + sz, bottom: top + sz };
            let ring = mix(p.sub, bgc, 35);
            let acc = if off { mix(p.sub, bgc, 50) } else if pressed { mix(p.accent, bgc, 20) } else { p.accent };
            let fill = if pressed { p.press } else { p.ctrl };
            let c = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
            // glifos da Segoe Fluent Icons: contornos suaves (anti-aliasing do ClearType)
            let g = |s: &str, col: u32, f: usize| { let mut r = bx; text(hdc, s, &mut r, col, fonts(f), c); };
            match (u & U_RADIO != 0, on) {
                (true, true) => { g("\u{E91F}", acc, 6); g("\u{E91F}", p.on_accent, 7); }
                (true, false) => { g("\u{E91F}", fill, 6); g("\u{ECCA}", ring, 6); }
                (false, true) => { g("\u{E73B}", acc, 6); g("\u{E73E}", p.on_accent, 5); }
                (false, false) => { g("\u{E73B}", fill, 6); g("\u{E739}", ring, 6); }
            }
            rc.left += d(32);
            let mut tr = rc;
            text(hdc, &label, &mut tr, txt, fonts(0), DT_LEFT | DT_SINGLELINE | DT_VCENTER);
            if focus {
                let mut m = rc;
                let old = SelectObject(hdc, fonts(0).into());
                DrawTextW(hdc, &mut wide(&label), &mut m, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
                SelectObject(hdc, old);
                let f = RECT { left: rc.left - d(6), top: rc.top + d(1), right: m.right + d(6), bottom: rc.bottom - d(1) };
                outline(hdc, &f, d(6), p.text, 1);
            }
            return;
        }
        let primary = u & U_PRIMARY != 0 && !off;
        let (fill, line, fg) = if primary {
            let f = if pressed { mix(p.accent, bgc, 25) } else { p.accent };
            (f, f, p.on_accent)
        } else {
            (if pressed { p.press } else { p.ctrl }, if focus { p.accent } else { mix(p.border, p.sub, 18) }, if pressed { p.sub } else { txt })
        };
        rr(hdc, &rc, d(8), fill, line);
        if focus && primary {
            let f = RECT { left: rc.left + d(2), top: rc.top + d(2), right: rc.right - d(2), bottom: rc.bottom - d(2) };
            outline(hdc, &f, d(6), p.on_accent, 1);
        }
        // rótulo só com um glifo (setas da paginação) usa a fonte de ícones
        let icon = label.chars().next().is_some_and(|c| ('\u{E000}'..='\u{F8FF}').contains(&c));
        text(hdc, &label, &mut rc, fg, if icon { fonts(5) } else { fonts(0) }, DT_CENTER | DT_SINGLELINE | DT_VCENTER);
    }
}
