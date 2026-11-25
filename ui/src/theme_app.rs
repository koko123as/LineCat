//! The theme page and the custom colour page on `App`'s side (lib/views/theme.dart,
//! lib/widgets/palette.dart).

use crate::colors::{self, STYLES};
use crate::theme::Palette;
use crate::{fonts, store, ActionItem, App, AppWindow, Scheme, Swatch, Theme, Type};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex, OnceLock};

/// What the custom colour page holds until it is confirmed.
#[derive(Clone, Copy, Default)]
struct Draft {
    /// The colour being edited; None adds one.
    target: Option<u32>,
    color: u32,
    /// The sliders' own values: a gray keeps the hue it was dragged to.
    hsv: (f64, f64, f64),
    /// Where the spectrum was last touched and what it picked there.
    spot: Option<(f64, f64, u32)>,
}

struct ThemeState {
    /// The system's light and dark seeds (`DynamicColorSeeds`), once the shell has said.
    system: Option<(u32, u32)>,
    draft: Draft,
    grid: Vec<u32>,
    /// The system's font scale (`Configuration.fontScale`), once the shell has said.
    font_scale: f32,
    /// The font page's families, read once.
    families: Option<Vec<String>>,
}

fn state() -> &'static Mutex<ThemeState> {
    static S: OnceLock<Mutex<ThemeState>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(ThemeState {
            system: None,
            draft: Draft::default(),
            grid: colors::grid(),
            font_scale: 1.0,
            families: None,
        })
    })
}

fn rgb(v: u32) -> slint::Color {
    slint::Color::from_argb_encoded(0xff00_0000 | v)
}

fn scheme(p: &Palette) -> Scheme {
    Scheme {
        primary: rgb(p.primary),
        on_primary: rgb(p.on_primary),
        primary_container: rgb(p.primary_container),
        on_primary_container: rgb(p.on_primary_container),
        page: rgb(p.page),
        card: rgb(p.card),
        block: rgb(p.block),
        inset: rgb(p.inset),
        on_surface: rgb(p.on_surface),
        on_surface_variant: rgb(p.on_surface_variant),
        outline_variant: rgb(p.outline_variant),
        error: rgb(p.error),
    }
}

/// Sets the app's schemes and what the theme page shows from the settings.
pub fn apply(ui: &AppWindow, settings: &store::Settings) {
    let (system, font_scale) = {
        let st = state().lock().unwrap();
        (st.system, st.font_scale)
    };
    let text = settings.text_scale;
    let scale = fonts::effective_scale(text.enable, text.scale, font_scale);
    let typ = ui.global::<Type>();
    typ.set_scale(scale);
    typ.set_family(settings.font_family.clone().unwrap_or_default().into());
    ui.set_text_scale_enable(text.enable);
    ui.set_text_scale(scale);
    let seeds = colors::seeds(settings.primary_color, system);
    let style = settings.color_style;
    let (light, dark) = colors::schemes(seeds, style, settings.pure_black);
    let theme = ui.global::<Theme>();
    theme.set_light(scheme(&light));
    theme.set_dark(scheme(&dark));
    theme.set_mode(settings.theme_mode);
    ui.set_theme_mode(settings.theme_mode);
    ui.set_pure_black(settings.pure_black);
    ui.set_floating_bar(settings.floating_navigation_bar);
    ui.set_tab_fade(settings.tab_animation == 1);
    let standard = colors::palette(seeds.1, style, true);
    ui.set_scheme_dark_standard(scheme(&standard));
    ui.set_scheme_dark_black(scheme(&colors::pure_black(standard, standard.page)));
    let (system_light, system_dark) = colors::seeds(None, system);
    let swatches: Vec<Swatch> = std::iter::once(None)
        .chain(settings.primary_colors.iter().map(|c| Some(*c)))
        .map(|c| {
            let (l, d) = c.map(|c| (c, c)).unwrap_or((system_light, system_dark));
            Swatch {
                fill: rgb(l),
                fill_dark: rgb(d),
                system: c.is_none(),
                selected: c == settings.primary_color,
                light_ink: colors::is_dark(l),
                light_ink_dark: colors::is_dark(d),
            }
        })
        .collect();
    ui.set_swatches(ModelRc::new(VecModel::from(swatches)));
    ui.set_colors_can_reset(!colors::is_default(
        &settings.primary_colors,
        settings.primary_color,
        style,
    ));
    ui.set_color_style_label(colors::style_label(style).into());
}

/// The custom colour page's properties for `draft`.
fn show_draft(ui: &AppWindow, settings: &store::Settings, draft: &Draft, grid: &[u32], from: From) {
    let color = draft.color;
    let light = colors::palette(color, settings.color_style, false);
    let mut dark = colors::palette(color, settings.color_style, true);
    if settings.pure_black {
        dark = colors::pure_black(dark, dark.page);
    }
    let (fx, fy) = match draft.spot {
        Some((x, y, c)) if c == color => (x, y),
        _ => {
            let (h, l) = colors::to_hl(color);
            (h / 360.0, 1.0 - l)
        }
    };
    let (h, s, v) = draft.hsv;
    ui.set_color_draft(rgb(color));
    ui.set_color_light(scheme(&light));
    ui.set_color_dark(scheme(&dark));
    if from != From::Hex {
        ui.set_color_hex(colors::hex(color)[1..].into());
    }
    if from == From::Open || ui.get_color_grid().row_count() == 0 {
        let model: Vec<slint::Color> = grid.iter().map(|c| rgb(*c)).collect();
        ui.set_color_grid(ModelRc::new(VecModel::from(model)));
    }
    let selected = grid.iter().position(|c| *c == color);
    ui.set_color_grid_selected(selected.map(|i| i as i32).unwrap_or(-1));
    ui.set_color_light_ink(colors::is_dark(color));
    ui.set_color_spec_x(fx as f32);
    ui.set_color_spec_y(fy as f32);
    ui.set_color_hue(h as f32);
    ui.set_color_sat(s as f32);
    ui.set_color_val(v as f32);
    ui.set_color_hue_color(rgb(colors::from_hsv(h, 1.0, 1.0)));
    ui.set_color_sat_from(rgb(colors::from_hsv(h, 0.0, v)));
    ui.set_color_sat_to(rgb(colors::from_hsv(h, 1.0, v)));
    ui.set_color_val_to(rgb(colors::from_hsv(h, s, 1.0)));
}

/// The custom colour page opened on `color`, for examples/snapshot.rs.
pub fn show_color_page(ui: &AppWindow, settings: &store::Settings, color: u32) {
    let draft = Draft {
        target: None,
        color,
        hsv: colors::to_hsv(color),
        spot: None,
    };
    show_draft(ui, settings, &draft, &colors::grid(), From::Open);
}

/// Which picker a change came from, so that picker keeps its own reading of the colour.
#[derive(PartialEq)]
enum From {
    Open,
    Hex,
    Grid,
    Spectrum,
    Sliders,
}

impl App {
    pub(crate) fn publish_theme(self: &Arc<Self>) {
        let settings = self.store.lock().unwrap().state.settings.clone();
        self.ui(move |ui| apply(&ui, &settings));
    }

    /// The system's font scale, for text that follows it.
    pub(crate) fn set_system_font_scale(self: &Arc<Self>, scale: f32) {
        let changed = {
            let mut st = state().lock().unwrap();
            let changed = (st.font_scale - scale).abs() > f32::EPSILON;
            st.font_scale = scale;
            changed
        };
        if changed {
            self.publish_theme();
        }
    }

    fn open_fonts(self: &Arc<Self>) {
        let cached = state().lock().unwrap().families.clone();
        let loading = cached.is_none();
        self.ui(move |ui| {
            ui.set_font_searching(false);
            ui.set_font_query("".into());
            ui.set_font_loading(loading);
            ui.set_tools_page("font".into());
        });
        match cached {
            Some(families) => self.show_fonts(families, ""),
            None => self.spawn(|app| {
                let families = fonts::system_families();
                state().lock().unwrap().families = Some(families.clone());
                app.show_fonts(families, "");
            }),
        }
    }

    fn show_fonts(self: &Arc<Self>, families: Vec<String>, query: &str) {
        let searchable = families.len() + 1 >= 10;
        let shown: Vec<SharedString> = fonts::matching(&families, query)
            .into_iter()
            .map(Into::into)
            .collect();
        self.ui(move |ui| {
            ui.set_font_searchable(searchable);
            ui.set_font_families(ModelRc::new(VecModel::from(shown)));
            ui.set_font_loading(false);
        });
    }

    /// The shell's hello: the system's seeds, for a theme that follows the system.
    pub(crate) fn set_system_seeds(self: &Arc<Self>, seeds: Option<(u32, u32)>) {
        state().lock().unwrap().system = seeds;
        self.publish_theme();
    }

    fn update_theme(self: &Arc<Self>, f: impl FnOnce(&mut store::Settings)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings);
            store.save();
        }
        self.publish_theme();
    }

    pub(crate) fn bind_theme(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_set_theme_mode(move |m| a.update_theme(|s| s.theme_mode = m));
        let a = self.clone();
        ui.on_set_pure_black(move |on| a.update_theme(|s| s.pure_black = on));
        let a = self.clone();
        ui.on_set_floating_bar(move |on| a.update_theme(|s| s.floating_navigation_bar = on));
        let a = self.clone();
        ui.on_set_tab_animation(move |i| a.update_theme(|s| s.tab_animation = i));
        let a = self.clone();
        ui.on_set_text_scale_enable(move |on| a.update_theme(|s| s.text_scale.enable = on));
        let a = self.clone();
        ui.on_set_text_scale(move |v| a.update_theme(|s| s.text_scale.scale = fonts::snap(v)));
        let a = self.clone();
        ui.on_open_font_page(move || a.open_fonts());
        let a = self.clone();
        ui.on_font_search(move |q| {
            let families = state().lock().unwrap().families.clone();
            if let Some(families) = families {
                a.show_fonts(families, &q);
            }
        });
        let a = self.clone();
        ui.on_font_pick(move |f| {
            let family = (!f.is_empty()).then(|| f.to_string());
            a.update_theme(|s| s.font_family = family);
        });
        let a = self.clone();
        ui.on_swatch_picked(move |i| {
            a.update_theme(|s| {
                s.primary_color = match i {
                    0 => None,
                    i => s
                        .primary_colors
                        .get(i as usize - 1)
                        .copied()
                        .or(s.primary_color),
                }
            })
        });
        let a = self.clone();
        ui.on_swatch_menu(move |i| a.swatch_menu(i));
        let a = self.clone();
        ui.on_add_color(move || {
            let initial = a.store.lock().unwrap().state.settings.primary_color;
            a.open_color(None, initial.unwrap_or(colors::DEFAULT_PRIMARY));
        });
        let a = self.clone();
        ui.on_reset_colors(move || {
            a.confirm(
                "提示",
                "确定要重置吗？".into(),
                "color-reset",
                String::new(),
            )
        });
        let a = self.clone();
        ui.on_pick_color_style(move || a.open_style_choice());
        ui.on_color_hex_filter(|t| colors::hex_digits(&t).into());
        let a = self.clone();
        ui.on_color_hex_edited(move |t| {
            if let Some(c) = colors::parse_hex(&t) {
                a.draft_set(c, From::Hex);
            }
        });
        let a = self.clone();
        ui.on_color_grid_pick(move |i| {
            let c = state().lock().unwrap().grid.get(i.max(0) as usize).copied();
            if let Some(c) = c {
                a.draft_set(c, From::Grid);
            }
        });
        let a = self.clone();
        ui.on_color_spectrum_pick(move |x, y| {
            let (x, y) = (x as f64, y as f64);
            let c = colors::spectrum_pick(x, y);
            state().lock().unwrap().draft.spot = Some((x, y, c));
            a.draft_set(c, From::Spectrum);
        });
        let a = self.clone();
        ui.on_color_hsv(move |h, s, v| {
            let hsv = (h as f64, s as f64, v as f64);
            state().lock().unwrap().draft.hsv = hsv;
            a.draft_set(colors::from_hsv(hsv.0, hsv.1, hsv.2), From::Sliders);
        });
        let a = self.clone();
        ui.on_color_confirm(move || a.confirm_color());
    }

    fn swatch_menu(self: &Arc<Self>, index: i32) {
        let colors = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .primary_colors
            .clone();
        let Some(color) = (index >= 1)
            .then(|| colors.get(index as usize - 1).copied())
            .flatten()
        else {
            return;
        };
        let item = |id: &str, glyph: &str, label: &str, danger: bool| ActionItem {
            id: format!("swatch:{id}:{color}").into(),
            glyph: glyph.into(),
            label: label.into(),
            danger,
            enabled: true,
            ..Default::default()
        };
        let items = vec![
            item("edit", "edit", "编辑", false),
            item("delete", "delete", "删除", true),
        ];
        let title: SharedString = colors::hex(color).into();
        self.ui(move |ui| {
            ui.set_choice_title(title);
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    fn open_style_choice(self: &Arc<Self>) {
        let style = self.store.lock().unwrap().state.settings.color_style;
        let items: Vec<ActionItem> = STYLES
            .iter()
            .enumerate()
            .map(|(i, label)| ActionItem {
                id: format!("color-style:{i}").into(),
                label: (*label).into(),
                check: true,
                checked: i as i32 == style,
                enabled: true,
                ..Default::default()
            })
            .collect();
        self.ui(move |ui| {
            ui.set_choice_title("配色风格".into());
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    /// The theme page's sheet choices; false when `key` is not one of them.
    pub(crate) fn theme_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match key {
            "color-style" => {
                if let Ok(style) = value.parse::<i32>() {
                    self.update_theme(|s| s.color_style = style);
                }
            }
            "swatch" => {
                let (action, color) = value.split_once(':').unwrap_or((value, ""));
                let Ok(color) = color.parse::<u32>() else {
                    return true;
                };
                match action {
                    "edit" => self.open_color(Some(color), color),
                    "delete" => self.confirm(
                        "提示",
                        "确定删除当前颜色吗？".into(),
                        "color-delete",
                        color.to_string(),
                    ),
                    _ => {}
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn theme_confirmed(self: &Arc<Self>, action: &str, arg: &str) {
        match action {
            "color-reset" => self.update_theme(|s| {
                s.primary_colors = crate::theme::ACCENTS.to_vec();
                s.primary_color = Some(colors::DEFAULT_PRIMARY);
                s.color_style = colors::VIVID;
            }),
            "color-delete" => {
                if let Ok(color) = arg.parse::<u32>() {
                    self.update_theme(|s| {
                        s.primary_color =
                            colors::delete(&mut s.primary_colors, s.primary_color, color)
                    });
                }
            }
            _ => {}
        }
    }

    fn open_color(self: &Arc<Self>, target: Option<u32>, initial: u32) {
        state().lock().unwrap().draft = Draft {
            target,
            color: initial,
            hsv: colors::to_hsv(initial),
            spot: None,
        };
        self.publish_color_draft(From::Open);
        self.ui(|ui| {
            ui.set_color_mode(0);
            ui.set_tools_page("theme-color".into());
        });
    }

    fn draft_set(self: &Arc<Self>, color: u32, from: From) {
        {
            let mut st = state().lock().unwrap();
            let d = &mut st.draft;
            d.color = color;
            let (h, s, v) = d.hsv;
            if from != From::Sliders && colors::from_hsv(h, s, v) != color {
                d.hsv = colors::to_hsv(color);
            }
        }
        self.publish_color_draft(from);
    }

    fn publish_color_draft(self: &Arc<Self>, from: From) {
        let (draft, grid) = {
            let st = state().lock().unwrap();
            (st.draft, st.grid.clone())
        };
        let settings = self.store.lock().unwrap().state.settings.clone();
        self.ui(move |ui| show_draft(&ui, &settings, &draft, &grid, from));
    }

    fn confirm_color(self: &Arc<Self>) {
        let draft = state().lock().unwrap().draft;
        let color = draft.color;
        self.update_theme(|s| {
            s.primary_color = match draft.target {
                None => colors::add(&mut s.primary_colors, color),
                Some(old) => colors::edit(&mut s.primary_colors, s.primary_color, old, color),
            };
        });
        self.ui(|ui| ui.set_tools_page("theme".into()));
    }
}
