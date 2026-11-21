//! Acesso HID (Power Device Class) a qualquer UPS USB que siga a norma (CyberPower, APC, Eaton, …).
use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::*;
use windows::Win32::Devices::HumanInterfaceDevice::*;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;

/// Páginas de usage da norma USB HID Power Device (0x84) e Battery System (0x85).
pub const PAGE_POWER: u16 = 0x84;
pub const PAGE_BATTERY: u16 = 0x85;

/// Nome do fabricante pelo vendor ID USB (quando a UPS não o diz).
pub fn vendor_name(vid: u16) -> &'static str {
    match vid {
        0x0764 => "CyberPower", 0x051D => "APC", 0x0463 => "Eaton", 0x0592 => "Powerware", 0x06DA => "Phoenixtec",
        0x0D9F => "Powercom", 0x09AE => "Tripp Lite", 0x050D => "Belkin", 0x10AF => "Liebert",
        _ => "",
    }
}

pub struct Hid {
    pub handle: HANDLE,
    pub pp: PHIDP_PREPARSED_DATA,
    pub caps: HIDP_CAPS,
    pub path: String,
    pub writable: bool,
    pub vid: u16,
    pub pid: u16,
    /// Localização dos valores no descritor desta UPS (calculada uma vez).
    pub map: std::sync::OnceLock<crate::ups::Map>,
}

pub fn find_paths() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let guid = HidD_GetHidGuid();
        let Ok(set) = SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) else { return out };
        let mut i = 0;
        loop {
            let mut ifd = SP_DEVICE_INTERFACE_DATA { cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInterfaces(set, None, &guid, i, &mut ifd).is_err() { break; }
            i += 1;
            let mut need = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &ifd, None, 0, Some(&mut need), None);
            let mut buf = vec![0u8; need as usize];
            let det = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*det).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &ifd, Some(det), need, None, None).is_ok() {
                let p = (*det).DevicePath.as_ptr();
                let mut n = 0;
                while *p.add(n) != 0 { n += 1; }
                out.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

// O handle HID suporta pedidos concorrentes (o driver serializa-os).
unsafe impl Send for Hid {}
unsafe impl Sync for Hid {}

impl Hid {
    pub fn open(path: &str) -> Option<Hid> {
        unsafe {
            let w: Vec<u16> = path.encode_utf16().chain([0]).collect();
            let share = FILE_SHARE_READ | FILE_SHARE_WRITE;
            let (h, writable) = match CreateFileW(PCWSTR(w.as_ptr()), 0xC0000000, share, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None) {
                Ok(h) => (h, true),
                Err(_) => (CreateFileW(PCWSTR(w.as_ptr()), 0, share, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None).ok()?, false),
            };
            let mut at = HIDD_ATTRIBUTES { Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
            if !HidD_GetAttributes(h, &mut at) {
                let _ = CloseHandle(h);
                return None;
            }
            let mut pp = PHIDP_PREPARSED_DATA::default();
            if !HidD_GetPreparsedData(h, &mut pp) {
                let _ = CloseHandle(h);
                return None;
            }
            let mut caps = HIDP_CAPS::default();
            let _ = HidP_GetCaps(pp, &mut caps);
            // só dispositivos da classe Power Device / Battery System (UPS de qualquer marca)
            if caps.UsagePage != PAGE_POWER && caps.UsagePage != PAGE_BATTERY {
                let _ = HidD_FreePreparsedData(pp);
                let _ = CloseHandle(h);
                return None;
            }
            Some(Hid { handle: h, pp, caps, path: path.to_string(), writable, vid: at.VendorID, pid: at.ProductID, map: Default::default() })
        }
    }

    pub fn open_ups() -> Option<Hid> {
        find_paths().iter().find_map(|p| Hid::open(p))
    }

    pub fn value_caps(&self, rt: HIDP_REPORT_TYPE) -> Vec<HIDP_VALUE_CAPS> {
        let n = match rt {
            HidP_Input => self.caps.NumberInputValueCaps,
            HidP_Output => self.caps.NumberOutputValueCaps,
            _ => self.caps.NumberFeatureValueCaps,
        };
        let mut v = vec![HIDP_VALUE_CAPS::default(); n as usize];
        let mut len = n;
        unsafe { let _ = HidP_GetValueCaps(rt, v.as_mut_ptr(), &mut len, self.pp); }
        v.truncate(len as usize);
        v
    }

    pub fn button_caps(&self, rt: HIDP_REPORT_TYPE) -> Vec<HIDP_BUTTON_CAPS> {
        let n = match rt {
            HidP_Input => self.caps.NumberInputButtonCaps,
            HidP_Output => self.caps.NumberOutputButtonCaps,
            _ => self.caps.NumberFeatureButtonCaps,
        };
        let mut v = vec![HIDP_BUTTON_CAPS::default(); n as usize];
        let mut len = n;
        unsafe { let _ = HidP_GetButtonCaps(rt, v.as_mut_ptr(), &mut len, self.pp); }
        v.truncate(len as usize);
        v
    }

    pub fn link_nodes(&self) -> Vec<HIDP_LINK_COLLECTION_NODE> {
        let mut len = self.caps.NumberLinkCollectionNodes as u32;
        let mut v = vec![HIDP_LINK_COLLECTION_NODE::default(); len as usize];
        unsafe { let _ = HidP_GetLinkCollectionNodes(v.as_mut_ptr(), &mut len, self.pp); }
        v.truncate(len as usize);
        v
    }

    /// Lê um feature report completo (ReportID no byte 0).
    pub fn get_feature(&self, id: u8) -> Option<Vec<u8>> {
        let mut b = vec![0u8; self.caps.FeatureReportByteLength as usize];
        if b.is_empty() { return None; }
        b[0] = id;
        unsafe {
            if HidD_GetFeature(self.handle, b.as_mut_ptr() as *mut c_void, b.len() as u32) { Some(b) } else { None }
        }
    }

    /// Lê um input report a pedido (para UPS que só expõem certos valores como input).
    pub fn get_input(&self, id: u8) -> Option<Vec<u8>> {
        let mut b = vec![0u8; self.caps.InputReportByteLength as usize];
        if b.is_empty() { return None; }
        b[0] = id;
        unsafe {
            if HidD_GetInputReport(self.handle, b.as_mut_ptr() as *mut c_void, b.len() as u32) { Some(b) } else { None }
        }
    }

    /// Texto USB: 0 fabricante, 1 produto, 2 n.º de série, ou um índice de string (>= 3).
    pub fn string(&self, which: u32) -> String {
        let mut b = [0u16; 128];
        let p = b.as_mut_ptr() as *mut c_void;
        let n = (b.len() * 2) as u32;
        let ok = unsafe {
            match which {
                0 => HidD_GetManufacturerString(self.handle, p, n),
                1 => HidD_GetProductString(self.handle, p, n),
                2 => HidD_GetSerialNumberString(self.handle, p, n),
                i => HidD_GetIndexedString(self.handle, i, p, n),
            }
        };
        if !ok { return String::new(); }
        let len = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..len]).trim().to_string()
    }

    pub fn set_feature(&self, rep: &mut [u8]) -> bool {
        unsafe { HidD_SetFeature(self.handle, rep.as_mut_ptr() as *mut c_void, rep.len() as u32) }
    }

    /// Valor bruto (não "scaled") de um usage dentro de um report.
    pub fn usage_value(&self, rt: HIDP_REPORT_TYPE, page: u16, link: u16, usage: u16, rep: &mut [u8]) -> Option<u32> {
        let mut v = 0u32;
        let st = unsafe { HidP_GetUsageValue(rt, page, Some(link), usage, &mut v, self.pp, rep) };
        if st == HIDP_STATUS_SUCCESS { Some(v) } else { None }
    }

    pub fn set_usage_value(&self, rt: HIDP_REPORT_TYPE, page: u16, link: u16, usage: u16, val: u32, rep: &mut [u8]) -> bool {
        unsafe { HidP_SetUsageValue(rt, page, Some(link), usage, val, self.pp, rep) == HIDP_STATUS_SUCCESS }
    }
}

impl Drop for Hid {
    fn drop(&mut self) {
        unsafe {
            let _ = HidD_FreePreparsedData(self.pp);
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Hid {
    /// Botões ativos (usages) de um report, dentro de uma página/coleção.
    pub fn usages(&self, rt: HIDP_REPORT_TYPE, page: u16, link: u16, rep: &mut [u8]) -> Vec<u16> {
        let mut buf = [0u16; 32];
        let mut n = buf.len() as u32;
        let st = unsafe { HidP_GetUsages(rt, page, Some(link), buf.as_mut_ptr(), &mut n, self.pp, rep) };
        if st == HIDP_STATUS_SUCCESS { buf[..n as usize].to_vec() } else { vec![] }
    }
}
