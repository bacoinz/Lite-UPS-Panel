//! Modelo de alto nível da UPS sobre o HID, genérico para qualquer marca.
//! Cada valor é procurado no descritor pelo seu nome standard (usage da norma USB Power Device),
//! e não por números de relatório fixos: assim funciona com CyberPower, APC, Eaton, etc.
use crate::hid::{vendor_name, Hid, PAGE_BATTERY as PG_BS, PAGE_POWER as PG_PD};
use windows::Win32::Devices::HumanInterfaceDevice::*;

// usages das coleções (página Power Device)
const C_STATUS: u16 = 0x02; // PresentStatus
const C_INPUT: u16 = 0x1A; // Input
const C_OUTPUT: u16 = 0x1C; // Output

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub ac_present: bool,
    pub charging: bool,
    pub discharging: bool,
    pub fully_charged: bool,
    pub below_cap_limit: bool,
    pub time_limit_expired: bool,
    pub boost: bool,
    pub buck: bool,
    pub overload: bool,
    pub capacity: u32,
    pub runtime_s: u32,
    pub in_v: u32,
    pub out_v: u32,
    pub load_pct: u32,
    pub nominal_w: u32,
    pub test: u8,
    pub alarm: u8,
    pub low_tr: u32,
    pub high_tr: u32,
}

impl Status {
    pub fn load_w(&self) -> u32 { self.load_pct * self.nominal_w / 100 }
    /// Em bateria: a UPS indica descarga e/ou sem AC.
    pub fn on_battery(&self) -> bool { self.discharging || !self.ac_present }
}

pub struct Limits { pub low: (u32, u32), pub high: (u32, u32) }

/// Onde está um valor: tipo de relatório, id, página/usage e coleção, mais unidade e limites.
#[derive(Clone, Copy, Debug)]
pub struct Loc { rt: HIDP_REPORT_TYPE, id: u8, page: u16, link: u16, usage: u16, unit: u32, exp: i32, lmin: i32, lmax: i32 }

/// Mapa do descritor desta UPS (o que ela suporta e onde).
#[derive(Debug, Default)]
pub struct Map {
    pub name: String,
    capacity: Option<Loc>, runtime: Option<Loc>, in_v: Option<Loc>, out_v: Option<Loc>, load: Option<Loc>,
    nominal_w: Option<Loc>, test: Option<Loc>, alarm: Option<Loc>, low_tr: Option<Loc>, high_tr: Option<Loc>,
    ac: Option<Loc>, charging: Option<Loc>, discharging: Option<Loc>, full: Option<Loc>, below: Option<Loc>,
    expired: Option<Loc>, boost: Option<Loc>, buck: Option<Loc>, overload: Option<Loc>,
}

impl Map {
    pub fn supports_transfer(&self) -> bool { self.low_tr.is_some() && self.high_tr.is_some() }
    pub fn supports_alarm(&self) -> bool { self.alarm.is_some_and(|l| l.rt == HidP_Feature) }
    pub fn supports_test(&self) -> bool { self.test.is_some_and(|l| l.rt == HidP_Feature) }
}

/// Mapa (calculado na primeira utilização e guardado no próprio Hid).
pub fn map(h: &Hid) -> &Map { h.map.get_or_init(|| build_map(h)) }

/// A coleção `link` (ou uma das que a contêm) tem este usage?
fn in_coll(nodes: &[HIDP_LINK_COLLECTION_NODE], link: u16, usage: u16) -> bool {
    let mut l = link as usize;
    for _ in 0..16 {
        let Some(n) = nodes.get(l) else { return false };
        if n.LinkUsagePage == PG_PD && n.LinkUsage == usage { return true; }
        if l == 0 { return false; }
        l = n.Parent as usize;
    }
    false
}

fn build_map(h: &Hid) -> Map {
    let nodes = h.link_nodes();
    // feature primeiro (lê-se a pedido de forma fiável); input como alternativa
    let mut vals: Vec<(HIDP_REPORT_TYPE, HIDP_VALUE_CAPS)> = Vec::new();
    let mut btns: Vec<(HIDP_REPORT_TYPE, HIDP_BUTTON_CAPS)> = Vec::new();
    for rt in [HidP_Feature, HidP_Input] {
        vals.extend(h.value_caps(rt).into_iter().map(|c| (rt, c)));
        btns.extend(h.button_caps(rt).into_iter().map(|c| (rt, c)));
    }
    let range = |is: bool, a: (u16, u16), b: u16| if is { (a.0, a.1) } else { (b, b) };
    // `coll`: coleção exigida (ex.: tensão de entrada só dentro de Input); `avoid`: preferir fora destas
    let val = |page: u16, usage: u16, coll: Option<u16>, avoid: &[u16]| -> Option<Loc> {
        let mut best: Option<(i32, Loc)> = None;
        for (rt, c) in &vals {
            let (lo, hi) = unsafe { range(c.IsRange, (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax), c.Anonymous.NotRange.Usage) };
            if c.UsagePage != page || usage < lo || usage > hi { continue; }
            if let Some(k) = coll { if !in_coll(&nodes, c.LinkCollection, k) { continue; } }
            let mut score = if *rt == HidP_Feature { 2 } else { 0 };
            if !avoid.iter().any(|a| in_coll(&nodes, c.LinkCollection, *a)) { score += 1; }
            let e = c.UnitsExp as i32;
            let loc = Loc { rt: *rt, id: c.ReportID, page, link: c.LinkCollection, usage, unit: c.Units, exp: if e > 7 { e - 16 } else { e },
                lmin: c.LogicalMin, lmax: c.LogicalMax };
            if best.as_ref().is_none_or(|(s, _)| score > *s) { best = Some((score, loc)); }
        }
        best.map(|b| b.1)
    };
    let btn = |page: u16, usage: u16, prefer: Option<u16>| -> Option<Loc> {
        let mut best: Option<(i32, Loc)> = None;
        for (rt, c) in &btns {
            let (lo, hi) = unsafe { range(c.IsRange, (c.Anonymous.Range.UsageMin, c.Anonymous.Range.UsageMax), c.Anonymous.NotRange.Usage) };
            if c.UsagePage != page || usage < lo || usage > hi { continue; }
            let mut score = if *rt == HidP_Feature { 2 } else { 0 };
            if prefer.is_some_and(|k| in_coll(&nodes, c.LinkCollection, k)) { score += 1; }
            let loc = Loc { rt: *rt, id: c.ReportID, page, link: c.LinkCollection, usage, unit: 0, exp: 0, lmin: 0, lmax: 1 };
            if best.as_ref().is_none_or(|(s, _)| score > *s) { best = Some((score, loc)); }
        }
        best.map(|b| b.1)
    };
    let io = [C_INPUT, C_OUTPUT];
    let mut m = Map {
        name: String::new(),
        capacity: val(PG_BS, 0x66, None, &io),
        runtime: val(PG_BS, 0x68, None, &io),
        in_v: val(PG_PD, 0x30, Some(C_INPUT), &[]),
        out_v: val(PG_PD, 0x30, Some(C_OUTPUT), &[]),
        load: val(PG_PD, 0x35, Some(C_OUTPUT), &[]).or_else(|| val(PG_PD, 0x35, None, &[])),
        nominal_w: val(PG_PD, 0x44, None, &[C_INPUT]),
        test: val(PG_PD, 0x58, None, &[]),
        alarm: val(PG_PD, 0x5A, None, &[]),
        low_tr: val(PG_PD, 0x53, None, &[C_OUTPUT]),
        high_tr: val(PG_PD, 0x54, None, &[C_OUTPUT]),
        ac: btn(PG_BS, 0xD0, Some(C_STATUS)),
        charging: btn(PG_BS, 0x44, Some(C_STATUS)),
        discharging: btn(PG_BS, 0x45, Some(C_STATUS)),
        full: btn(PG_BS, 0x46, Some(C_STATUS)),
        below: btn(PG_BS, 0x42, Some(C_STATUS)),
        expired: btn(PG_BS, 0x43, Some(C_STATUS)),
        boost: btn(PG_PD, 0x6E, None),
        buck: btn(PG_PD, 0x6F, None),
        overload: btn(PG_PD, 0x65, None),
    };
    m.name = device_name(h, &val(PG_PD, 0xFD, None, &[]), &val(PG_PD, 0xFE, None, &[]));
    m
}

/// "Marca Modelo": textos USB e, se faltarem, as strings indicadas pela própria UPS (iManufacturer/iProduct).
fn device_name(h: &Hid, i_mfr: &Option<Loc>, i_prod: &Option<Loc>) -> String {
    let mut r = Reader::new(h);
    let indexed = |r: &mut Reader, l: &Option<Loc>| l.and_then(|l| r.raw(&l)).filter(|&i| i > 0).map(|i| h.string(i)).unwrap_or_default();
    let mut maker = h.string(0);
    if maker.is_empty() { maker = indexed(&mut r, i_mfr); }
    if maker.is_empty() || maker.eq_ignore_ascii_case("cps") { maker = vendor_name(h.vid).to_string(); }
    let mut model = indexed(&mut r, i_prod);
    if model.is_empty() { model = h.string(1); }
    // APC acrescenta versões de firmware ao nome ("Back-UPS ES 700G FW:871...")
    if let Some(i) = model.find(" FW:") { model.truncate(i); }
    let model = model.trim();
    match (maker.is_empty(), model.is_empty()) {
        (true, true) => format!("UPS USB {:04X}:{:04X}", h.vid, h.pid),
        (true, false) => model.to_string(),
        (false, true) => maker,
        (false, false) if model.to_lowercase().starts_with(&maker.to_lowercase()) => model.to_string(),
        _ => format!("{maker} {model}"),
    }
}

/// Lê cada relatório uma só vez por ciclo (cada leitura USB custa ~150 ms).
struct Reader<'a> { h: &'a Hid, cache: Vec<(HIDP_REPORT_TYPE, u8, Option<Vec<u8>>)> }

impl<'a> Reader<'a> {
    fn new(h: &'a Hid) -> Self { Reader { h, cache: Vec::new() } }
    fn report(&mut self, l: &Loc) -> Option<&mut Vec<u8>> {
        let i = match self.cache.iter().position(|c| c.0 == l.rt && c.1 == l.id) {
            Some(i) => i,
            None => {
                let r = if l.rt == HidP_Feature { self.h.get_feature(l.id) } else { self.h.get_input(l.id) };
                self.cache.push((l.rt, l.id, r));
                self.cache.len() - 1
            }
        };
        self.cache[i].2.as_mut()
    }
    fn raw(&mut self, l: &Loc) -> Option<u32> {
        let h = self.h;
        let rep = self.report(l)?;
        h.usage_value(l.rt, l.page, l.link, l.usage, rep)
    }
    /// Valor em unidades naturais: volts e watts são corrigidos pelo expoente da unidade.
    fn get(&mut self, l: &Option<Loc>) -> Option<u32> {
        let l = l.as_ref()?;
        let v = self.raw(l)?;
        Some(scale(l, v))
    }
    fn bit(&mut self, l: &Option<Loc>) -> bool {
        let Some(l) = l.as_ref() else { return false };
        let h = self.h;
        self.report(l).is_some_and(|rep| h.usages(l.rt, l.page, l.link, rep).contains(&l.usage))
    }
}

/// Volt (0xF0D121) e watt (0xD121) usam expoente 7 como "unidade inteira" (sistema SI linear em cm/g).
fn scale(l: &Loc, v: u32) -> u32 {
    if l.unit != 0x00F0_D121 && l.unit != 0x0000_D121 { return v; }
    let e = l.exp - 7;
    if e >= 0 { v.saturating_mul(10u32.pow(e as u32)) } else { v / 10u32.pow((-e) as u32) }
}
fn unscale(l: &Loc, v: u32) -> u32 {
    if l.unit != 0x00F0_D121 && l.unit != 0x0000_D121 { return v; }
    let e = l.exp - 7;
    if e >= 0 { v / 10u32.pow(e as u32) } else { v.saturating_mul(10u32.pow((-e) as u32)) }
}

pub fn read_status(h: &Hid) -> Option<Status> {
    let m = map(h);
    let mut r = Reader::new(h);
    let capacity = r.get(&m.capacity)?; // se falhar, sem comunicação
    let ac_present = if m.ac.is_some() { r.bit(&m.ac) } else { true };
    Some(Status {
        ac_present,
        charging: r.bit(&m.charging),
        discharging: r.bit(&m.discharging),
        fully_charged: r.bit(&m.full),
        below_cap_limit: r.bit(&m.below),
        time_limit_expired: r.bit(&m.expired),
        boost: r.bit(&m.boost),
        buck: r.bit(&m.buck),
        overload: r.bit(&m.overload),
        capacity,
        runtime_s: r.get(&m.runtime).unwrap_or(0),
        in_v: r.get(&m.in_v).unwrap_or(0),
        out_v: r.get(&m.out_v).unwrap_or(0),
        load_pct: r.get(&m.load).unwrap_or(0),
        nominal_w: r.get(&m.nominal_w).unwrap_or(0),
        test: r.get(&m.test).unwrap_or(6) as u8,
        alarm: r.get(&m.alarm).unwrap_or(0) as u8,
        low_tr: r.get(&m.low_tr).unwrap_or(0),
        high_tr: r.get(&m.high_tr).unwrap_or(0),
    })
}

/// Intervalos permitidos pela própria UPS (LogicalMin/Max); None se não for configurável.
pub fn limits(h: &Hid) -> Option<Limits> {
    let m = map(h);
    let (lo, hi) = (m.low_tr?, m.high_tr?);
    let s = |l: &Loc| (scale(l, l.lmin.max(0) as u32), scale(l, l.lmax.max(0) as u32));
    Some(Limits { low: s(&lo), high: s(&hi) })
}

fn write(h: &Hid, l: &Option<Loc>, v: u32) -> bool {
    let Some(l) = l else { return false };
    if l.rt != HidP_Feature { return false; }
    let Some(mut rep) = h.get_feature(l.id) else { return false };
    h.set_usage_value(HidP_Feature, l.page, l.link, l.usage, unscale(l, v), &mut rep) && h.set_feature(&mut rep)
}

pub fn set_transfer(h: &Hid, low: u32, high: u32) -> bool {
    let Some(lim) = limits(h) else { return false };
    if low < lim.low.0 || low > lim.low.1 || high < lim.high.0 || high > lim.high.1 { return false; }
    let m = map(h);
    let (a, b) = (write(h, &m.low_tr, low), write(h, &m.high_tr, high));
    let mut r = Reader::new(h);
    a && b && r.get(&m.low_tr) == Some(low) && r.get(&m.high_tr) == Some(high)
}

/// AudibleAlarmControl da norma: 1 desativado, 2 ativado.
pub fn set_alarm(h: &Hid, enabled: bool) -> bool {
    let v = if enabled { 2 } else { 1 };
    let m = map(h);
    write(h, &m.alarm, v) && Reader::new(h).get(&m.alarm) == Some(v)
}

/// Test da norma: 1 = teste rápido.
pub fn start_self_test(h: &Hid) -> bool { write(h, &map(h).test, 1) }
