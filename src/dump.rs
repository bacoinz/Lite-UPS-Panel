use crate::hid::*;
use windows::Win32::Devices::HumanInterfaceDevice::*;

pub fn run() {
    let paths = find_paths();
    println!("HID interfaces: {}", paths.len());
    for p in &paths {
        let Some(h) = Hid::open(p) else { continue };
        println!(
            "\n=== {}\nwritable={} usagePage={:#x} usage={:#x} in={} out={} feat={} links={}",
            h.path, h.writable, h.caps.UsagePage, h.caps.Usage,
            h.caps.InputReportByteLength, h.caps.OutputReportByteLength,
            h.caps.FeatureReportByteLength, h.caps.NumberLinkCollectionNodes
        );
        for (i, n) in h.link_nodes().iter().enumerate() {
            println!("  link[{i}] page={:#x} usage={:#x} parent={}", n.LinkUsagePage, n.LinkUsage, n.Parent);
        }
        for (name, rt) in [("INPUT", HidP_Input), ("FEATURE", HidP_Feature)] {
            for c in h.value_caps(rt) {
                let (u, ur) = unsafe {
                    if c.IsRange { (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax) }
                    else { (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage) }
                };
                println!(
                    "  {name} VAL rid={} page={:#x} usage={:#x}-{:#x} link={} bits={} cnt={} lmin={} lmax={} unit={:#x} exp={}",
                    c.ReportID, c.UsagePage, u, ur, c.LinkCollection, c.BitSize, c.ReportCount,
                    c.LogicalMin, c.LogicalMax, c.Units, c.UnitsExp
                );
            }
            for c in h.button_caps(rt) {
                let (u, ur) = unsafe {
                    if c.IsRange { (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax) }
                    else { (c.Anonymous.NotRange.Usage, c.Anonymous.NotRange.Usage) }
                };
                println!("  {name} BTN rid={} page={:#x} usage={:#x}-{:#x} link={}", c.ReportID, c.UsagePage, u, ur, c.LinkCollection);
            }
        }
        let mut ids: Vec<u8> = h.value_caps(HidP_Feature).iter().map(|c| c.ReportID).collect();
        ids.extend(h.button_caps(HidP_Feature).iter().map(|c| c.ReportID));
        ids.sort();
        ids.dedup();
        for id in ids {
            match h.get_feature(id) {
                Some(mut r) => {
                    let mut line = format!("  FEATURE rid={id} raw={:02x?}\n", r);
                    for c in h.value_caps(HidP_Feature).iter().filter(|c| c.ReportID == id && unsafe { !c.IsRange }) {
                        let u = unsafe { c.Anonymous.NotRange.Usage };
                        if let Some(v) = h.usage_value(HidP_Feature, c.UsagePage, c.LinkCollection, u, &mut r) {
                            line += &format!("      {:#x}:{:#x} link={} = {}\n", c.UsagePage, u, c.LinkCollection, v);
                        }
                    }
                    print!("{line}");
                }
                None => println!("  FEATURE rid={id} GetFeature failed"),
            }
        }
    }
}

/// Reescreve o valor atual do alarme (rid 128) para provar que SetFeature funciona sem elevação.
pub fn write_test() {
    let Some(h) = Hid::open_ups() else { println!("UPS not found"); return };
    let Some(mut r) = h.get_feature(128) else { println!("get failed"); return };
    println!("before: {:02x?} writable={}", r, h.writable);
    let ok = h.set_feature(&mut r);
    let after = h.get_feature(128);
    println!("SetFeature ok={ok} after={:02x?}", after);
}

/// `--shot <pasta>`: abre a janela, percorre as 5 páginas em cada idioma e grava BMPs (para rever o layout).
pub fn shot(dir: &str) {
    use crate::{i18n, nav, ui, ui_win};
    use windows::Win32::Foundation::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::*;
    std::env::set_var("UPSPANEL_DRYRUN", "1");
    let _ = std::fs::create_dir_all(dir);
    if !crate::app::shot_setup() { println!("sem app"); return; }
    let pump = |ms: u64| unsafe {
        let t = std::time::Instant::now();
        let mut m = MSG::default();
        while t.elapsed().as_millis() < ms as u128 {
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() { let _ = TranslateMessage(&m); DispatchMessageW(&m); }
            crate::app::shot_tick();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    ui::open(HWND::default());
    pump(4000);
    let main = ui::hwnd_of(&ui::MAIN);
    if main.is_invalid() { println!("sem janela"); return; }
    for (li, code) in i18n::CODES.iter().enumerate() {
        i18n::set(i18n::resolve(code));
        unsafe { SendMessageW(main, ui_win::WM_RELANG, None, None); }
        for page in 0..5usize {
            if ui::CUR.get() != page { unsafe { SendMessageW(main, nav::WM_NAV_SELECT, Some(WPARAM(page)), None); } }
            pump(1500);
            unsafe {
                let mut rc = RECT::default();
                let _ = GetWindowRect(main, &mut rc);
                let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
                let _ = RedrawWindow(Some(main), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW);
                let dc = GetDC(None);
                let mem = CreateCompatibleDC(Some(dc));
                let bmp = CreateCompatibleBitmap(dc, w, h);
                let old = SelectObject(mem, bmp.into());
                let _ = PrintWindow(main, mem, PRINT_WINDOW_FLAGS(2));
                let mut bi = BITMAPINFO::default();
                bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, ..Default::default() };
                let mut px = vec![0u8; (w * h * 4) as usize];
                GetDIBits(mem, bmp, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
                SelectObject(mem, old);
                let _ = DeleteObject(bmp.into());
                let _ = DeleteDC(mem);
                ReleaseDC(None, dc);
                // BMP 32 bpp, de cima para baixo
                let mut f = Vec::with_capacity(px.len() + 54);
                f.extend_from_slice(b"BM");
                f.extend_from_slice(&(54 + px.len() as u32).to_le_bytes());
                f.extend_from_slice(&[0, 0, 0, 0]);
                f.extend_from_slice(&54u32.to_le_bytes());
                f.extend_from_slice(&40u32.to_le_bytes());
                f.extend_from_slice(&w.to_le_bytes());
                f.extend_from_slice(&(-h).to_le_bytes());
                f.extend_from_slice(&1u16.to_le_bytes());
                f.extend_from_slice(&32u16.to_le_bytes());
                f.extend_from_slice(&[0u8; 24]);
                f.extend_from_slice(&px);
                let _ = std::fs::write(format!("{dir}/{}_{}.bmp", i18n::CODES[li], page), f);
            }
        }
    }
    println!("ok");
}
