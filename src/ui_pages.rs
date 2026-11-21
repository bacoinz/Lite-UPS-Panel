//! As 5 páginas da janela: construção, atualização periódica e tratamento de comandos.
use crate::i18n::{self, fill, tr};
use crate::app::unix;
use crate::monitor::Mode;
use crate::ui::*;
use crate::ups::{self, Status};
use windows::core::w;
use windows::Win32::Foundation::HWND;

const CO2_LB: f64 = 2.20462;
const KINDS: [&str; 15] = [
    "PowerFail", "UnderVoltage", "OverVoltage", "Boost", "Buck", "RegulationEnd", "LowBattery", "CommLost", "CommRestored",
    "PowerRestored", "SelfTestStart", "SelfTestPass", "SelfTestFail", "ShutdownStart", "ShutdownCancel",
];

pub fn kind_label(k: &str) -> &'static str {
    match k {
        "PowerFail" => tr("Falha de energia"), "UnderVoltage" => tr("Subtensão"), "OverVoltage" => tr("Sobretensão"),
        "Boost" => tr("Regulação Boost"), "Buck" => tr("Regulação Buck"), "RegulationEnd" => tr("Fim de regulação"),
        "LowBattery" => tr("Bateria fraca"), "CommLost" => tr("Comunicação com a UPS perdida"),
        "CommRestored" => tr("Comunicação com a UPS restabelecida"), "PowerRestored" => tr("Energia restabelecida"),
        "SelfTestStart" => tr("Auto-teste iniciado"), "SelfTestPass" => tr("Auto-teste concluído: passou"),
        "SelfTestFail" => tr("Auto-teste concluído: falhou"), "ShutdownStart" => tr("Desligar o computador iniciado"),
        "ShutdownCancel" => tr("Desligar o computador cancelado"), "ShutdownFailed" => tr("Falha ao desligar o computador"),
        _ => tr("Evento"),
    }
}

struct Country { name: String, code: String, kg: f64, lb: f64, cost: f64, sym: String }

fn tag<'a>(b: &'a str, t: &str) -> &'a str {
    let (o, c) = (format!("<{t}>"), format!("</{t}>"));
    b.find(&o).and_then(|i| b[i + o.len()..].find(&c).map(|j| &b[i + o.len()..i + o.len() + j])).unwrap_or("")
}

fn countries() -> Vec<Country> {
    let xml = include_str!("../data/countries.xml");
    let mut v: Vec<Country> = xml.split("<country name=\"").skip(1).filter_map(|blk| {
        let name = blk.split('"').next()?.to_string();
        let cur = tag(blk, "currency");
        let sym = cur.split(|c: char| c.is_ascii_digit()).next().unwrap_or("").trim().to_string();
        Some(Country { name, code: tag(blk, "countryCode").to_string(), kg: tag(blk, "co2EmittedKg").parse().ok()?,
            lb: tag(blk, "co2EmittedLb").parse().unwrap_or(0.0), cost: tag(blk, "cost").parse().ok()?, sym })
    }).collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

fn num(s: &str) -> Option<f64> { s.trim().replace(',', ".").parse().ok() }
fn fnum(v: f64, dec: usize) -> String { format!("{v:.dec$}").replace('.', &i18n::dec().to_string()) }
/// Número sem zeros finais (ex.: 0,0900 -> 0,09).
fn ftrim(v: f64, dec: usize) -> String { fnum(v, dec).trim_end_matches('0').trim_end_matches(i18n::dec()).to_string() }

fn snapshot() -> (Option<Status>, (u32, u32)) {
    with_app(|a| (a.st.clone(), (a.st.as_ref().map(|s| s.low_tr).unwrap_or(0), a.st.as_ref().map(|s| s.high_tr).unwrap_or(0)))).unwrap_or((None, (0, 0)))
}

// ============ construção ============
pub fn build(i: usize, p: HWND) {
    match i {
        0 => build_status(p),
        1 => build_events(p),
        2 => build_energy(p),
        3 => build_config(p),
        _ => build_ups(p),
    }
    refresh(i);
}

fn build_status(p: HWND) {
    for (i, cap) in [tr("Fonte de energia"), tr("Capacidade da bateria"), tr("Autonomia estimada")].iter().enumerate() {
        let x = 24 + i as i32 * 226;
        crate::theme::card_init(1001 + i as i32, cap, ["\u{E7E8}", "\u{E83F}", "\u{E916}"][i]);
        mk(p, w!("STATIC"), "—", x, 16, 210, 100, 1001 + i as i32, 0xD, 0, 2);
    }
    heading(p, tr("Estado atual"), 24, 136);
    let rows = [tr("Marca e modelo"), tr("Energia fornecida por"), tr("Tensão de saída"), tr("Condição da energia de entrada"), tr("Condição da energia de saída"),
        tr("Capacidade restante da bateria"), tr("Estado da bateria"), tr("Autonomia restante"), tr("Carga da UPS"), tr("Estado do hardware")];
    let divs: Vec<i32> = (1..rows.len() as i32).map(|i| 172 + i * 34).collect();
    panel(24, 172, 662, rows.len() as i32 * 34, &divs);
    for (i, r) in rows.iter().enumerate() {
        let y = 172 + i as i32 * 34;
        row_label(p, r, 44, y, 250, 34, 0, 0);
        // a marca/modelo (1.ª linha) usa o id 1019; as restantes mantêm 1010..1018
        row_label(p, "—", 300, y, 370, 34, if i == 0 { 1019 } else { 1009 + i as i32 }, 3);
    }
}

fn build_events(p: HWND) {
    heading(p, tr("Resumo de problemas de energia"), 24, 12);
    row_label(p, tr("Período"), 470, 12, 52, 28, 0, 0);
    combo(p, 526, 14, 160, 1120, &[tr("Última semana").into(), tr("Último mês").into(), tr("Último ano").into()], 0);
    panel(24, 52, 662, 206, &[]);
    listview(p, 29, 57, 652, 196, 1121, &[(tr("Problema de energia"), 250), (tr("Número de vezes"), 170), (tr("Duração"), 0)]);

    heading(p, tr("Registo de eventos"), 24, 278);
    let today = unix();
    row_label(p, tr("De"), 24, 314, 44, 26, 0, 0);
    dtp(p, 68, 315, 1101, today - 6 * 86400);
    row_label(p, tr("Até"), 186, 314, 44, 26, 0, 0);
    dtp(p, 230, 315, 1102, today);
    let mut kinds = vec![tr("Todos").to_string()];
    kinds.extend(KINDS.iter().map(|k| kind_label(k).to_string()));
    combo(p, 352, 315, 166, 1103, &kinds, 0);
    let e = edit(p, "", 526, 315, 78, 1104);
    cue(e, tr("Filtrar…"));
    button(p, tr("Procurar"), 612, 313, 74, 1105);
    panel(24, 354, 662, 186, &[]);
    listview(p, 29, 359, 652, 176, 1106, &[(tr("Hora"), 170), (tr("Evento"), 0)]);
    button(p, "\u{E76B}", 24, 554, 34, 1107);
    let pg = row_label(p, "1 / 1", 60, 554, 50, 30, 1109, 0);
    center(pg);
    button(p, "\u{E76C}", 112, 554, 34, 1108);
    combo(p, 160, 557, 60, 1110, &["10".into(), "20".into(), "50".into()], 0);
    row_label(p, tr("por página"), 228, 554, 100, 30, 0, 0);
    button(p, tr("Apagar eventos do período"), 366, 554, 320, 1111);
    load_summary();
    load_events();
}

fn build_energy(p: HWND) {
    heading(p, tr("Relatório de energia"), 24, 12);
    let today = unix();
    row_label(p, tr("De"), 24, 50, 44, 26, 0, 0);
    dtp(p, 68, 51, 1201, today);
    row_label(p, tr("Até"), 186, 50, 44, 26, 0, 0);
    dtp(p, 230, 51, 1202, today);
    button(p, tr("Atualizar"), 360, 49, 110, 1203);
    let rows = [tr("Consumo médio diário"), tr("Consumo acumulado"), tr("Custo acumulado"), tr("CO2 emitido")];
    let divs: Vec<i32> = (1..rows.len() as i32).map(|i| 92 + i * 34).collect();
    panel(24, 92, 662, rows.len() as i32 * 34, &divs);
    for (i, r) in rows.iter().enumerate() {
        let y = 92 + i as i32 * 34;
        row_label(p, r, 44, y, 250, 34, 0, 0);
        row_label(p, "—", 300, y, 370, 34, 1210 + i as i32, 3);
    }

    heading(p, tr("Definições de energia"), 24, 248);
    let cs = countries();
    let names: Vec<String> = cs.iter().map(|c| format!("{} - {}", c.code, c.name)).collect();
    let (region, cost, co2, unit) = with_app(|a| (a.db.get("region"), a.cost_kwh, a.co2_kwh, a.db.get("co2_unit").unwrap_or_else(|| "kg".into()))).unwrap();
    let idx = region.and_then(|r| cs.iter().position(|c| c.name == r)).or_else(|| cs.iter().position(|c| c.name == "PORTUGAL")).unwrap_or(0);
    let y0 = 284;
    panel(24, y0, 662, 4 * 44, &[y0 + 44, y0 + 88, y0 + 132]);
    row_label(p, tr("Região"), 44, y0, 220, 44, 0, 0);
    combo(p, 280, y0 + 10, 300, 1220, &names, idx);
    row_label(p, tr("Custo por kWh"), 44, y0 + 44, 220, 44, 0, 0);
    edit(p, &ftrim(cost, 4), 280, y0 + 54, 120, 1221);
    row_label(p, tr("CO2 emitido por kWh"), 44, y0 + 88, 220, 44, 0, 0);
    let lb = unit == "lb";
    edit(p, &fnum(if lb { co2 * CO2_LB } else { co2 }, 3), 280, y0 + 98, 120, 1222);
    row_label(p, tr("Unidade de medida"), 44, y0 + 132, 220, 44, 0, 0);
    combo(p, 280, y0 + 142, 180, 1223, &[tr("Quilogramas (kg)").into(), tr("Libras (lb)").into()], lb as usize);
    button(p, tr("Aplicar"), 24, 476, 110, 1224);
    row_label(p, "", 146, 476, 300, 30, 1225, 0);
    load_energy();
}

fn build_config(p: HWND) {
    let (sounds, mode, st) = with_app(|a| (a.sounds, a.mode, a.st.clone())).unwrap();
    heading(p, tr("Notificações"), 24, 12);
    panel(24, 48, 662, 162, &[116]);
    row_label(p, tr("Sons de software"), 44, 54, 300, 26, 0, 0);
    check(p, tr("Ativar sons de notificação"), 44, 82, 360, 1301);
    set_check(1301, sounds);
    row_label(p, tr("Alarmes da UPS"), 44, 122, 300, 26, 0, 0);
    radio(p, tr("Ativar alarmes sempre"), 44, 150, 360, 1302, true);
    radio(p, tr("Desativar alarmes sempre"), 44, 176, 360, 1303, false);
    let alarm_on = st.as_ref().map(|s| s.alarm != 1).unwrap_or(true);
    set_check(1302, alarm_on);
    set_check(1303, !alarm_on);
    if !with_app(|a| a.hid.as_ref().is_some_and(|h| ups::map(h).supports_alarm())).unwrap_or(false) {
        enable(1302, false);
        enable(1303, false);
    }

    heading(p, tr("Configuração de autonomia"), 24, 232);
    panel(24, 268, 662, 156, &[346]);
    radio(p, tr("Manter o computador a funcionar — desligar quando a autonomia restante for"), 44, 280, 630, 1304, true);
    let mins: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
    combo(p, 76, 310, 70, 1305, &mins, match mode { Mode::KeepRunning(n) => n as usize - 1, _ => 4 });
    row_label(p, tr("minutos"), 156, 308, 80, 28, 0, 0);
    radio(p, tr("Preservar a bateria — desligar o computador após estar em bateria durante"), 44, 358, 630, 1306, false);
    let pm: Vec<String> = (0..=60).map(|n| n.to_string()).collect();
    combo(p, 76, 388, 70, 1307, &pm, match mode { Mode::Preserve(n) => n as usize, _ => 0 });
    row_label(p, tr("minutos"), 156, 386, 80, 28, 0, 0);
    let keep = matches!(mode, Mode::KeepRunning(_));
    set_check(1304, keep);
    set_check(1306, !keep);
    button(p, tr("Aplicar"), 24, 442, 110, 1308);
    row_label(p, "", 146, 442, 300, 30, 1309, 0);

    heading(p, tr("Idioma"), 24, 490);
    panel(24, 526, 662, 44, &[]);
    row_label(p, tr("Idioma da aplicação"), 44, 526, 220, 44, 0, 0);
    let mut langs = vec![tr("Automático (idioma do sistema)").to_string()];
    langs.extend(i18n::NAMES.iter().map(|s| s.to_string()));
    let cur = with_app(|a| a.db.get("lang").unwrap_or_else(|| "auto".into())).unwrap_or_else(|| "auto".into());
    combo(p, 280, 536, 300, 1310, &langs, i18n::setting_index(&cur));
}

fn build_ups(p: HWND) {
    heading(p, tr("Tensão"), 24, 12);
    panel(24, 48, 662, 148, &[108, 152]);
    mk(p, w!("STATIC"), "", 44, 58, 620, 42, 1401, 0, 0, 0);
    let lim = with_app(|a| a.hid.as_ref().and_then(|h| ups::limits(h))).flatten();
    let (lo, hi) = lim.as_ref().map(|l| (l.low, l.high)).unwrap_or(((167, 175), (287, 295)));
    let st = snapshot().0;
    let lows: Vec<String> = (lo.0..=lo.1).map(|v| v.to_string()).collect();
    let highs: Vec<String> = (hi.0..=hi.1).map(|v| v.to_string()).collect();
    let cur_lo = st.as_ref().map(|s| (s.low_tr.clamp(lo.0, lo.1) - lo.0) as usize).unwrap_or(0);
    let cur_hi = st.as_ref().map(|s| (s.high_tr.clamp(hi.0, hi.1) - hi.0) as usize).unwrap_or(highs.len() - 1);
    row_label(p, tr("Quando a tensão da rede desce abaixo de"), 44, 108, 320, 44, 0, 0);
    combo(p, 380, 118, 80, 1402, &lows, cur_lo);
    row_label(p, tr("volts"), 470, 108, 60, 44, 0, 0);
    row_label(p, tr("Quando a tensão da rede sobe acima de"), 44, 152, 320, 44, 0, 0);
    combo(p, 380, 162, 80, 1403, &highs, cur_hi);
    row_label(p, tr("volts"), 470, 152, 60, 44, 0, 0);
    button(p, tr("Aplicar"), 24, 212, 110, 1404);
    row_label(p, "", 146, 212, 260, 30, 1405, 0);

    heading(p, tr("Auto-teste"), 24, 268);
    panel(24, 304, 662, 68, &[338]);
    row_label(p, tr("Data do último auto-teste"), 44, 304, 250, 34, 0, 0);
    row_label(p, "—", 300, 304, 370, 34, 1410, 3);
    row_label(p, tr("Resultado do último auto-teste"), 44, 338, 250, 34, 0, 0);
    row_label(p, "—", 300, 338, 370, 34, 1411, 3);
    button(p, tr("Iniciar auto-teste"), 24, 388, 160, 1412);
    // funções que esta UPS não suporta ficam desativadas (em vez de falharem ao clicar)
    let (has_tr, test) = with_app(|a| a.hid.as_ref().map(|h| (ups::map(h).supports_transfer(), ups::map(h).supports_test()))).flatten().unwrap_or((false, false));
    if !has_tr {
        for id in [1402, 1403, 1404] { enable(id, false); }
        set_text(1405, tr("Não suportado por esta UPS"));
    }
    if !test { enable(1412, false); }
}

// ============ atualização periódica ============
pub fn refresh(i: usize) {
    let (st, lim) = snapshot();
    match i {
        0 => refresh_status(&st, lim),
        4 => {
            let t = match &st {
                Some(s) => fill(tr("A tensão atual é de {} volts.\nA UPS intervém quando a tensão da rede desce abaixo de {} volts ou sobe acima de {} volts."), &[s.in_v.to_string(), s.low_tr.to_string(), s.high_tr.to_string()]),
                None => tr("Sem comunicação com a UPS.").into(),
            };
            set_text(1401, &t);
            let last = with_app(|a| a.db.last_test()).flatten();
            if st.as_ref().map(|s| s.test) == Some(5) {
                set_text(1411, tr("Em curso…"));
            } else if let Some((ts, ok)) = last {
                set_text(1410, &fmt_ts(ts));
                set_text(1411, if ok { tr("Passou") } else { tr("Falhou") });
            } else {
                set_text(1410, "—");
                set_text(1411, "—");
            }
        }
        _ => {}
    }
}

fn refresh_status(st: &Option<Status>, lim: (u32, u32)) {
    let Some(s) = st else {
        for id in 1001..=1003 { set_text(id, "—"); }
        for id in 1010..=1019 { set_text(id, "—"); }
        set_text(1010, tr("Sem comunicação com a UPS"));
        return;
    };
    let name = with_app(|a| a.hid.as_ref().map(|h| ups::map(h).name.clone())).flatten().unwrap_or_else(|| "—".into());
    set_text(1019, &name);
    let src = if s.on_battery() { tr("Bateria") } else { tr("Rede elétrica") };
    set_text(1001, src);
    set_text(1002, &format!("{}%", s.capacity));
    set_text(1003, &format!("{} min.", s.runtime_s / 60));
    let input = if s.on_battery() { tr("Falha de energia") }
        else if s.in_v > 0 && s.in_v < lim.0 { tr("Subtensão") } else if s.in_v > lim.1 { tr("Sobretensão") }
        else if s.boost { "Normal (Boost)" } else if s.buck { "Normal (Buck)" } else { tr("Normal") };
    let batt = if s.fully_charged { tr("Totalmente carregada") } else if s.charging { tr("A carregar") } else if s.discharging { tr("A descarregar") } else { "—" };
    let v = [
        src.to_string(), fill(tr("{} volts"), &[s.out_v.to_string()]), input.to_string(), (if s.overload { tr("Sobrecarga") } else { tr("Normal") }).to_string(),
        format!("{} %", s.capacity), batt.to_string(), fill(tr("{} minutos"), &[(s.runtime_s / 60).to_string()]),
        if s.nominal_w > 0 { format!("{}% ({} watts)", s.load_pct, s.load_w()) } else { format!("{}%", s.load_pct) }, tr("Normal").to_string(),
    ];
    for (i, t) in v.iter().enumerate() { set_text(1010 + i as i32, t); }
}

// ============ eventos ============
fn load_summary() {
    let secs = [7, 30, 365][sel(1120).min(2)] * 86400;
    let now = unix();
    let pr = with_app(|a| a.db.problems(now - secs, now).unwrap_or_default()).unwrap_or_default();
    let get = |k: &str| pr.iter().find(|p| p.kind == k).map(|p| (p.count, p.secs)).unwrap_or((0, 0));
    let row = |name: &str, (n, s): (i64, i64)| vec![name.to_string(), if n == 0 { tr("Nunca").into() } else { n.to_string() }, if n == 0 { tr("Nenhum").into() } else { fmt_dur(s) }];
    let (pf, uv, ov, bo, bu) = (get("PowerFail"), get("UnderVoltage"), get("OverVoltage"), get("Boost"), get("Buck"));
    let inv = (pf.0 + uv.0 + ov.0, pf.1 + uv.1 + ov.1);
    let reg = (bo.0 + bu.0, bo.1 + bu.1);
    lv_set(item(1121), &[row(tr("Falha de energia"), pf), row(tr("Subtensão"), uv), row(tr("Sobretensão"), ov), row(tr("Total em bateria"), inv),
        row("Boost", bo), row("Buck", bu), row(tr("Total de regulação"), reg)]);
}

fn load_events() {
    let (from, to) = (dtp_day(1101), dtp_day(1102) + 86400);
    let k = sel(1103);
    let kind = if k == 0 { "" } else { KINDS[k - 1] };
    let q = get_text(1104).to_lowercase();
    let size = [10usize, 20, 50][sel(1110).min(2)];
    let rows: Vec<(i64, String)> = with_app(|a| a.db.events(from, to, kind, "", 100_000, 0).map(|r| r.0).unwrap_or_default())
        .unwrap_or_default().into_iter()
        .map(|e| (e.ts, kind_label(&e.kind).to_string()))
        .filter(|(_, l)| q.is_empty() || l.to_lowercase().contains(&q)).collect();
    let pages = rows.len().div_ceil(size).max(1);
    let cur = EV_PAGE.get().min(pages - 1);
    EV_PAGE.set(cur);
    let slice: Vec<Vec<String>> = rows.iter().skip(cur * size).take(size).map(|(t, l)| vec![fmt_ts(*t), l.clone()]).collect();
    lv_set(item(1106), &slice);
    set_text(1109, &format!("{} / {}", cur + 1, pages));
}

// ============ energia ============
fn load_energy() {
    let (from, to) = (dtp_day(1201), dtp_day(1202) + 86400);
    let (r, sym, lb) = with_app(|a| (a.db.energy_report(from, to).unwrap_or_default(), a.db.get("currency").unwrap_or_else(|| "€".into()), a.db.get("co2_unit").as_deref() == Some("lb"))).unwrap();
    set_text(1210, &format!("{} kWh", fnum(r.avg_kwh_day, 2)));
    set_text(1211, &format!("{} kWh", fnum(r.kwh, 2)));
    set_text(1212, &format!("{} {}", sym, fnum(r.cost, 2)));
    set_text(1213, &format!("{} {}", fnum(if lb { r.co2 * CO2_LB } else { r.co2 }, 3), if lb { "lb" } else { "kg" }));
}

fn fill_region() {
    let cs = countries();
    let Some(c) = cs.get(sel(1220)) else { return };
    let lb = sel(1223) == 1;
    set_text(1221, &ftrim(c.cost, 4));
    set_text(1222, &fnum(if lb { c.lb } else { c.kg }, 3));
}

fn apply_energy() {
    let cs = countries();
    let lb = sel(1223) == 1;
    let (Some(cost), Some(co2)) = (num(&get_text(1221)), num(&get_text(1222))) else { set_text(1225, tr("Valores inválidos")); return };
    let kg = if lb { co2 / CO2_LB } else { co2 };
    let c = cs.get(sel(1220));
    with_app(|a| {
        a.cost_kwh = cost;
        a.co2_kwh = kg;
        let _ = a.db.set("cost_kwh", &cost.to_string());
        let _ = a.db.set("co2_kwh", &kg.to_string());
        let _ = a.db.set("co2_unit", if lb { "lb" } else { "kg" });
        if let Some(c) = c {
            let _ = a.db.set("region", &c.name);
            let _ = a.db.set("currency", if c.sym.is_empty() { "$" } else { &c.sym });
        }
    });
    set_text(1225, tr("Aplicado com sucesso"));
    load_energy();
}

// ============ comandos ============
pub fn command(page: usize, id: i32, code: u32) {
    const BN_CLICKED: u32 = 0;
    const CBN_SELCHANGE: u32 = 1;
    match (page, id) {
        (1, 1105) | (1, 1103) | (1, 1110) if code == BN_CLICKED || code == CBN_SELCHANGE => { EV_PAGE.set(0); load_events(); }
        (1, 1120) if code == CBN_SELCHANGE => load_summary(),
        (1, 1107) if code == BN_CLICKED => { EV_PAGE.set(EV_PAGE.get().saturating_sub(1)); load_events(); }
        (1, 1108) if code == BN_CLICKED => { EV_PAGE.set(EV_PAGE.get() + 1); load_events(); }
        (1, 1111) if code == BN_CLICKED => {
            let (from, to) = (dtp_day(1101), dtp_day(1102) + 86400);
            if confirm(tr("Apagar permanentemente os eventos do período selecionado?")) {
                with_app(|a| { let _ = a.db.delete_events(from, to); });
                load_events();
                load_summary();
            }
        }
        (2, 1203) if code == BN_CLICKED => load_energy(),
        (2, 1220) if code == CBN_SELCHANGE => fill_region(),
        (2, 1223) if code == CBN_SELCHANGE => {
            if let Some(v) = num(&get_text(1222)) {
                // converte o valor mostrado para a nova unidade
                let lb = sel(1223) == 1;
                set_text(1222, &fnum(if lb { v * CO2_LB } else { v / CO2_LB }, 3));
            }
        }
        (2, 1224) if code == BN_CLICKED => apply_energy(),
        (3, 1308) if code == BN_CLICKED => apply_config(),
        (3, 1310) if code == CBN_SELCHANGE => change_language(sel(1310)),
        (4, 1404) if code == BN_CLICKED => apply_voltage(),
        (4, 1412) if code == BN_CLICKED => {
            match with_app(|a| a.hid.clone()).flatten() {
                Some(h) => bg(move || ups::start_self_test(&h), |ok| msg(if ok { tr("Auto-teste iniciado. A UPS vai funcionar a bateria durante o teste.") } else { tr("Não foi possível iniciar o auto-teste.") })),
                None => msg(tr("Não foi possível iniciar o auto-teste.")),
            }
        }
        _ => {}
    }
}

fn confirm(text: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::*;
    let (t, c) = (wide(text), wide("Lite UPS Panel"));
    unsafe { MessageBoxW(Some(hwnd_of(&MAIN)), PCWSTR(t.as_ptr()), PCWSTR(c.as_ptr()), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

/// Muda o idioma já: grava, ativa e pede à janela principal para reconstruir a página.
fn change_language(i: usize) {
    let setting = i18n::setting_of_index(i);
    i18n::set(i18n::resolve(setting));
    with_app(|a| { let _ = a.db.set("lang", setting); });
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        let _ = PostMessageW(Some(hwnd_of(&MAIN)), crate::ui_win::WM_RELANG, WPARAM(0), LPARAM(0));
    }
}

fn apply_config() {
    let keep = checked(1304);
    let mode = if keep { Mode::KeepRunning(sel(1305) as u32 + 1) } else { Mode::Preserve(sel(1307) as u32) };
    let sounds = checked(1301);
    let alarm = checked(1302);
    let hw = with_app(|a| {
        a.mode = mode;
        a.sounds = sounds;
        let _ = a.db.set("sounds", if sounds { "1" } else { "0" });
        let _ = a.db.set("runtime_mode", if keep { "keep" } else { "preserve" });
        let _ = a.db.set("runtime_min", &match mode { Mode::KeepRunning(n) | Mode::Preserve(n) => n }.to_string());
        let cur = a.st.as_ref().map(|s| s.alarm != 1).unwrap_or(alarm);
        if cur != alarm { Some(a.hid.clone()) } else { None }
    }).flatten();
    match hw {
        None => set_text(1309, tr("Aplicado com sucesso")),
        Some(None) => set_text(1309, tr("Falha ao alterar os alarmes da UPS")),
        Some(Some(h)) => {
            set_text(1309, tr("A aplicar…"));
            bg(move || ups::set_alarm(&h, alarm), |ok| set_text(1309, if ok { tr("Aplicado com sucesso") } else { tr("Falha ao alterar os alarmes da UPS") }));
        }
    }
}

fn apply_voltage() {
    let lim = with_app(|a| a.hid.as_ref().and_then(|h| ups::limits(h))).flatten();
    let Some(l) = lim else { set_text(1405, tr("Não suportado por esta UPS")); return };
    let low = l.low.0 + sel(1402) as u32;
    let high = l.high.0 + sel(1403) as u32;
    let Some(h) = with_app(|a| a.hid.clone()).flatten() else { set_text(1405, tr("Sem comunicação")); return };
    set_text(1405, tr("A aplicar…"));
    bg(move || ups::set_transfer(&h, low, high), |ok| set_text(1405, if ok { tr("Aplicado com sucesso") } else { tr("Falha ao aplicar") }));
}
