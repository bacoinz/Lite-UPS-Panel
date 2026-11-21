#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod db;
mod dump;
mod hid;
mod monitor;
mod shutdown;
mod ui;
mod ui_pages;
mod ui_win;
mod theme;
mod nav;
mod ups;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    if has("--dump") { dump::run(); return; }
    if has("--write-test") { dump::write_test(); return; }
    if has("--status") {
        let h = hid::Hid::open_ups().expect("UPS");
        let lim = ups::limits(&h).map(|l| (l.low, l.high));
        println!("{}\n{:#?}\nlimits={:?}\n{:#?}", ups::map(&h).name, ups::read_status(&h), lim, ups::map(&h));
        return;
    }
    theme::init();
    app::run(has("--hidden"));
}
