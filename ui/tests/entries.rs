//! Every row on every settings page reached from 工具 does something when tapped: the page
//! moves, a sheet or dialog opens, a toast shows or the row itself changes. Snapshots set
//! the page directly, so a row whose tap reaches no handler would pass them unnoticed.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter};
use slint::{Model, Rgb8Pixel};
use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const W: u32 = 412;
/// Pages drawn by their own views rather than the settings rows: reaching them is the check.
const OWN_PAGES: &[&str] = &[
    "access",
    "editor",
    "font",
    "icon-set",
    "logs",
    "on-demand",
    "requests",
    "resources",
    "theme",
    "theme-color",
];
const H: u32 = 892;

type Job = Box<dyn FnOnce() + Send>;
static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static CLOCK_MS: AtomicU64 = AtomicU64::new(0);

struct Queue;

impl EventLoopProxy for Queue {
    fn quit_event_loop(&self) -> Result<(), slint::EventLoopError> {
        Ok(())
    }

    fn invoke_from_event_loop(&self, event: Job) -> Result<(), slint::EventLoopError> {
        JOBS.lock().unwrap().push(event);
        Ok(())
    }
}

struct Headless;

thread_local! {
    static WINDOW: std::cell::RefCell<Option<Rc<MinimalSoftwareWindow>>> = const { std::cell::RefCell::new(None) };
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        let w = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        w.set_size(slint::PhysicalSize::new(W, H));
        WINDOW.with(|c| *c.borrow_mut() = Some(w.clone()));
        Ok(w)
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(Queue))
    }

    fn duration_since_start(&self) -> Duration {
        Duration::from_millis(CLOCK_MS.load(Ordering::SeqCst))
    }
}

/// Runs what the handlers queued for the UI thread, lets their threads finish, and moves the
/// clock past every animation.
fn settle() {
    for _ in 0..6 {
        let jobs: Vec<Job> = std::mem::take(&mut *JOBS.lock().unwrap());
        let idle = jobs.is_empty();
        for job in jobs {
            job();
        }
        // Past the page and sheet animations, short of a toast's going.
        CLOCK_MS.fetch_add(100, Ordering::SeqCst);
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(if idle { 60 } else { 10 }));
    }
}

fn frame() -> Vec<Rgb8Pixel> {
    let mut buf = vec![Rgb8Pixel::default(); (W * H) as usize];
    WINDOW.with(|c| {
        let w = c.borrow().clone().unwrap();
        w.request_redraw();
        w.draw_if_needed(|r| {
            r.render(&mut buf, W as usize);
        });
    });
    buf
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn fresh() -> AppWindow {
    let home = std::env::temp_dir().join(format!(
        "seasoncat-entries-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let ui = headless(home).unwrap();
    ui.show().unwrap();
    ui.set_tab(3);
    settle();
    ui
}

/// The window on 工具 with [page] open.
fn on_page(page: &str) -> AppWindow {
    let ui = fresh();
    if !page.is_empty() {
        ui.set_tools_page(page.into());
        settle();
    }
    ui
}

struct Tap {
    page: String,
    id: String,
    kind: String,
    title: String,
    changed: bool,
    lands: String,
}

/// Taps [id] on [page] ("" for the 工具 list) and reports what moved.
fn tap(page: &str, id: &str, kind: &str, title: &str, on: bool) -> Tap {
    let ui = on_page(page);
    let before = frame();
    if page.is_empty() {
        ui.invoke_tools_clicked(id.into());
    } else if kind == "toggle" || kind == "switch" {
        ui.invoke_set_toggled(page.into(), id.into(), !on);
    } else {
        ui.invoke_set_clicked(page.into(), id.into());
    }
    settle();
    let after = frame();
    // A switch below the fold: its row turns.
    let turned = (kind == "toggle" || kind == "switch")
        && rows(&ui, page)
            .iter()
            .any(|(rid, _, _, now)| rid == id && *now != on);
    // Picking what is already picked leaves it so, as FlClash.
    let picked = kind == "check" && on;
    Tap {
        page: page.into(),
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        changed: before != after || turned || picked,
        lands: ui.get_tools_page().into(),
    }
}

fn rows(ui: &AppWindow, page: &str) -> Vec<(String, String, String, bool)> {
    let sections = if page.is_empty() {
        ui.get_tools_sections()
    } else {
        ui.get_page_sections()
    };
    let mut out = Vec::new();
    for s in sections.iter() {
        for r in s.rows.iter() {
            out.push((r.id.into(), r.kind.into(), r.title.into(), r.on));
        }
    }
    out
}

#[test]
fn every_settings_entry_leads_somewhere() {
    slint::platform::set_platform(Box::new(Headless)).unwrap();
    let mut queue: VecDeque<String> = VecDeque::from([String::new()]);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut taps = Vec::new();
    while let Some(page) = queue.pop_front() {
        if !seen.insert(page.clone()) || OWN_PAGES.contains(&page.as_str()) {
            continue;
        }
        let ui = on_page(&page);
        let mut entries = rows(&ui, &page);
        if !page.is_empty() {
            let meta = ui.invoke_page_meta(page.as_str().into(), 0);
            if !meta.action.is_empty() {
                entries.push(("action".into(), "bar".into(), meta.action.into(), false));
            }
            if !meta.action2.is_empty() {
                entries.push(("action2".into(), "bar".into(), meta.action2.into(), false));
            }
        }
        drop(ui);
        for (id, kind, title, on) in entries {
            let t = tap(&page, &id, &kind, &title, on);
            if t.lands != page && !t.lands.is_empty() {
                queue.push_back(t.lands.clone());
            }
            taps.push(t);
        }
    }
    let mut report = String::from("page\tid\tkind\ttitle\tchanged\tlands\n");
    for t in &taps {
        report.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            t.page, t.id, t.kind, t.title, t.changed, t.lands
        ));
    }
    if let Ok(path) = std::env::var("ENTRIES_REPORT") {
        std::fs::write(path, &report).unwrap();
    }
    let dead: Vec<String> = taps
        .iter()
        .filter(|t| !t.changed)
        .map(|t| format!("{} / {} ({}, {})", t.page, t.id, t.title, t.kind))
        .collect();
    assert!(
        dead.is_empty(),
        "taps that do nothing:\n{}",
        dead.join("\n")
    );
}
