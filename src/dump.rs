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
