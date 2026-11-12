mod preferences;
use eframe::egui;
use preferences::{Preferences, ThemeChoice};
use sissy::i18n::{language, set_language, text, Language};
use sissy::*;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sissy_set_icon_blob(kind: i32, bytes: *const u8, len: usize);
    fn sissy_apply_icon_appearance();
    fn sissy_start_icon_observer();
}

#[cfg(target_os = "macos")]
fn install_icon_appearances() {
    let plates: [(i32, &[u8]); 6] = [
        (0, include_bytes!("../assets/icon-default.png")),
        (1, include_bytes!("../assets/icon-dark.png")),
        (2, include_bytes!("../assets/icon-clear-light.png")),
        (3, include_bytes!("../assets/icon-clear-dark.png")),
        (4, include_bytes!("../assets/icon-clear-light.png")),
        (5, include_bytes!("../assets/icon-tinted-dark.png")),
    ];
    unsafe {
        for (kind, bytes) in plates {
            sissy_set_icon_blob(kind, bytes.as_ptr(), bytes.len());
        }
        sissy_start_icon_observer();
        sissy_apply_icon_appearance();
    }
}

#[cfg(target_os = "macos")]
fn refresh_icon_appearance() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 350 {
        return;
    }
    LAST.store(now, Ordering::Relaxed);
    unsafe { sissy_apply_icon_appearance() }
}
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
};
#[derive(Default)]
struct App {
    page: usize,
    preferences: Preferences,
    mascot: Option<egui::TextureHandle>,
    roots: Vec<PathBuf>,
    large: u64,
    days: u64,
    advanced: bool,
    report: Option<Report>,
    selected: HashSet<usize>,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
    receiver: Option<mpsc::Receiver<Report>>,
    confirm: bool,
    outcomes: Vec<String>,
    action_paths: Vec<PathBuf>,
    paths_in_log: bool,
    log_confirm: bool,
    status: String,
    focus: u8,
    disk: String,
    space_filter: u8,
}
impl App {
    fn persist_preferences(&mut self) {
        self.preferences.language = language();
        self.preferences.large = self.large;
        self.preferences.days = self.days;
        if let Err(e) = self.preferences.save() {
            self.status = sissy::tf!("Não foi possível salvar os ajustes: {e}", e = e);
        }
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let previous = self.preferences.clone();
        let previous_language = language();
        card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(title(text("Ajustes"), 26.));
            ui.label(text(
                "As alterações são aplicadas e salvas automaticamente.",
            ));
            ui.add_space(12.);
            ui.label(title(text("Idioma"), 18.));
            egui::ComboBox::from_id_salt("settings-language")
                .width(180.)
                .selected_text(self.preferences.language.name())
                .show_ui(ui, |ui| {
                    for choice in Language::ALL {
                        ui.selectable_value(&mut self.preferences.language, choice, choice.name());
                    }
                });
            ui.add_space(12.);
            ui.separator();
            ui.label(title(text("Aparência"), 18.));
            ui.horizontal_wrapped(|ui| {
                ui.label(text("Tema"));
                for (choice, label) in [
                    (ThemeChoice::System, text("Sistema")),
                    (ThemeChoice::Light, text("Claro")),
                    (ThemeChoice::Dark, text("Escuro")),
                ] {
                    ui.selectable_value(&mut self.preferences.theme, choice, label);
                }
            });
            ui.label(text("Tamanho da interface"));
            ui.add(
                egui::Slider::new(&mut self.preferences.scale, 0.9..=1.3)
                    .step_by(0.05)
                    .custom_formatter(|value, _| format!("{:.0}%", value * 100.)),
            );
            ui.add_space(12.);
            ui.separator();
            ui.label(title(text("Análise"), 18.));
            ui.horizontal_wrapped(|ui| {
                ui.label(text("Grandes (MiB)"));
                ui.add(egui::DragValue::new(&mut self.large).range(1..=1_000_000));
                ui.label(text("Downloads antigos (dias)"));
                ui.add(egui::DragValue::new(&mut self.days).range(1..=36_500));
            });
            ui.label(text(
                "Estes limites valem para a próxima análise. Resultados atuais não são alterados.",
            ));
            ui.add_space(12.);
            if ui.button(text("Restaurar padrões")).clicked() {
                // Preserve the user's language when restoring appearance and scan limits.
                self.preferences = Preferences {
                    language: self.preferences.language,
                    ..Default::default()
                };
                self.large = self.preferences.large;
                self.days = self.preferences.days;
            }
            ui.add_space(12.);
            ui.separator();
            ui.label(title(text("Créditos"), 18.));
            ui.label("© 2026 STRIKER Code Team");
            ui.hyperlink_to(
                "github.com/leonardobluestriker",
                "https://github.com/leonardobluestriker",
            );
            ui.small(format!("Sissy {}", env!("CARGO_PKG_VERSION")));
        });
        set_language(self.preferences.language);
        if language() != previous_language {
            self.disk = volume_line();
            self.status.clear();
            #[cfg(target_os = "macos")]
            install_mac_menu();
        }
        if self.preferences != previous
            || (self.large, self.days) != (self.preferences.large, self.preferences.days)
        {
            self.persist_preferences();
            apply_theme(ctx, self.preferences.theme);
            ctx.set_zoom_factor(self.preferences.scale);
        }
    }
    fn chosen(&self) -> Vec<Item> {
        self.report
            .as_ref()
            .map(|r| {
                self.selected
                    .iter()
                    .filter_map(|i| r.items.get(*i).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    fn start(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.progress.store(0, Ordering::Relaxed);
        let c = self.cancel.clone();
        let p = self.progress.clone();
        let o = Options {
            roots: self.roots.clone(),
            large: self.large.saturating_mul(1024 * 1024),
            old_days: self.days,
            duplicate_min: 1024 * 1024,
        };
        self.report = None;
        self.selected.clear();
        self.outcomes.clear();
        self.action_paths.clear();
        std::thread::spawn(move || {
            let r = scan(o, c, |n| {
                p.store(n, Ordering::Relaxed);
            });
            let _ = tx.send(r);
        });
    }
}
static DARK: AtomicBool = AtomicBool::new(false);
fn dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
fn surface() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(30, 33, 29)
    } else {
        egui::Color32::WHITE
    }
}
fn ink() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(238, 242, 232)
    } else {
        egui::Color32::BLACK
    }
}
fn border() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(100, 115, 86)
    } else {
        egui::Color32::BLACK
    }
}
fn muted() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(175, 185, 164)
    } else {
        egui::Color32::from_gray(80)
    }
}
fn canvas() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(19, 22, 18)
    } else {
        egui::Color32::from_gray(246)
    }
}
fn accent() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(48, 65, 15)
    } else {
        egui::Color32::from_rgb(204, 255, 0)
    }
}
fn soft_accent() -> egui::Color32 {
    if dark() {
        egui::Color32::from_rgb(35, 44, 24)
    } else {
        egui::Color32::from_rgb(247, 255, 214)
    }
}
fn apply_theme(ctx: &egui::Context, choice: ThemeChoice) {
    let is_dark = match choice {
        ThemeChoice::Dark => true,
        ThemeChoice::Light => false,
        ThemeChoice::System => ctx.system_theme() == Some(egui::Theme::Dark),
    };
    DARK.store(is_dark, Ordering::Relaxed);
    ctx.set_theme(if is_dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let mut visuals = if is_dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = surface();
    visuals.window_fill = surface();
    visuals.override_text_color = Some(ink());
    visuals.hyperlink_color = if is_dark {
        egui::Color32::from_rgb(204, 255, 80)
    } else {
        egui::Color32::from_rgb(50, 85, 0)
    };
    visuals.selection.bg_fill = accent();
    visuals.selection.stroke.color = ink();
    visuals.widgets.inactive.bg_fill = soft_accent();
    visuals.widgets.inactive.fg_stroke.color = ink();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.5_f32, border());
    visuals.widgets.hovered.bg_fill = accent();
    visuals.widgets.hovered.fg_stroke.color = ink();
    visuals.widgets.active.bg_fill = accent();
    visuals.widgets.active.fg_stroke.color = ink();
    ctx.set_visuals(visuals);
}
fn themed_image(source: egui::ImageSource<'_>) -> egui::Image<'_> {
    let source = if dark() {
        match source {
            egui::ImageSource::Bytes { uri, bytes } if uri.ends_with(".svg") => {
                let svg = String::from_utf8_lossy(bytes.as_ref()).replacen(
                    "<svg ",
                    "<svg fill=\"#ffffff\" ",
                    1,
                );
                egui::ImageSource::Bytes {
                    uri: format!("{uri}-dark.svg").into(),
                    bytes: svg.into_bytes().into(),
                }
            }
            other => other,
        }
    } else {
        source
    };
    egui::Image::new(source).tint(ink())
}
fn title(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(sissy::i18n::text(text))
        .size(size)
        .family(egui::FontFamily::Name("comic-bold".into()))
}
fn card() -> egui::Frame {
    egui::Frame::new()
        .shadow(egui::epaint::Shadow {
            offset: [4, 4],
            blur: 0,
            spread: 0,
            color: egui::Color32::from_black_alpha(90),
        })
        .fill(surface())
        .stroke(egui::Stroke::new(2_f32, border()))
        .corner_radius(12)
        .inner_margin(18)
}
fn format_when(modified: std::time::SystemTime) -> String {
    match modified.elapsed() {
        Ok(age) => {
            let days = age.as_secs() / 86_400;
            if days == 0 {
                sissy::i18n::text("modificado hoje").into()
            } else if days == 1 {
                sissy::i18n::text("modificado há 1 dia").into()
            } else {
                sissy::tf!("modificado há {days} dias", days = days)
            }
        }
        Err(_) => sissy::i18n::text("data de modificação no futuro").into(),
    }
}
fn matches_focus(item: &Item, focus: u8) -> bool {
    match focus {
        1 => item.tags.contains(&Tag::Large),
        2 => item.tags.contains(&Tag::Duplicate),
        3 => item.tags.contains(&Tag::OldDownload),
        4 => item.tags.contains(&Tag::Cache),
        5 => item.tags.contains(&Tag::Package),
        _ => true,
    }
}
fn volume_line() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let Ok(out) = std::process::Command::new("/bin/df")
        .args(["-k", &home])
        .output()
    else {
        return String::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let Some(line) = text.lines().nth(1) else {
        return String::new();
    };
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 4 {
        return String::new();
    }
    let total = cols[1].parse::<u64>().unwrap_or(0).saturating_mul(1024);
    let avail = cols[3].parse::<u64>().unwrap_or(0).saturating_mul(1024);
    sissy::tf!(
        "Neste disco: {:.1} GiB livres de {:.1} GiB",
        avail as f64 / 1_073_741_824.,
        total as f64 / 1_073_741_824.
    )
}
fn icon_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'_>,
    text: &str,
    green: bool,
    size: [f32; 2],
    icon_px: f32,
    text_px: f32,
) -> egui::Response {
    let label = title(text, text_px);
    let desired = text_width(
        ui,
        sissy::i18n::text(text),
        egui::FontId::new(text_px, egui::FontFamily::Name("comic-bold".into())),
    ) + icon_px
        + 34.;
    let width = desired
        .max(size[0].min(160.))
        .min(ui.available_width().max(48.));
    ui.add_sized(
        [width, size[1].min(40.)],
        egui::Button::image_and_text(
            themed_image(icon).fit_to_exact_size(egui::vec2(icon_px, icon_px)),
            label,
        )
        .fill(if green { accent() } else { surface() })
        .stroke(egui::Stroke::new(2_f32, border()))
        .corner_radius(10),
    )
}
fn finder_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::image_and_text(
            themed_image(egui::include_image!(
                "../assets/icons/folder-open-outline.svg"
            ))
            .fit_to_exact_size(egui::vec2(16., 16.)),
            "Finder",
        )
        .fill(surface())
        .stroke(egui::Stroke::new(2_f32, border()))
        .corner_radius(8),
    )
}
fn nav_icon(page: usize) -> egui::ImageSource<'static> {
    match page {
        0 => egui::include_image!("../assets/icons/sprout-outline.svg"),
        1 => egui::include_image!("../assets/icons/cog-outline.svg"),
        2 => egui::include_image!("../assets/icons/file-search-outline.svg"),
        3 => egui::include_image!("../assets/icons/clipboard-text-outline.svg"),
        5 => egui::include_image!("../assets/icons/cog-outline.svg"),
        _ => egui::include_image!("../assets/icons/snake.svg"),
    }
}
fn filter_icon(id: u8) -> egui::ImageSource<'static> {
    match id {
        1 => egui::include_image!("../assets/icons/target.svg"),
        2 => egui::include_image!("../assets/icons/content-copy.svg"),
        3 => egui::include_image!("../assets/icons/download-outline.svg"),
        4 => egui::include_image!("../assets/icons/power-plug-off-outline.svg"),
        5 => egui::include_image!("../assets/icons/movie-open-outline.svg"),
        _ => egui::include_image!("../assets/icons/magnify.svg"),
    }
}
#[cfg(target_os = "macos")]
fn install_mac_menu() {
    use muda::{
        accelerator::{Accelerator, Code, Modifiers},
        AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu,
    };
    let key = |code| Some(Accelerator::new(Some(Modifiers::SUPER), code));
    let shift = |code| {
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            code,
        ))
    };
    let about = AboutMetadata {
        name: Some("Sissy".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        copyright: Some("© 2026 STRIKER Code Team".into()),
        credits: Some("github.com/leonardobluestriker".into()),
        ..Default::default()
    };
    let about_item =
        PredefinedMenuItem::about(Some(sissy::i18n::text("Sobre o Sissy")), Some(about));
    let settings = MenuItem::with_id("settings", text("Ajustes…"), true, key(Code::Comma));
    let services = PredefinedMenuItem::services(Some(sissy::i18n::text("Serviços")));
    let hide = PredefinedMenuItem::hide(Some(sissy::i18n::text("Ocultar Sissy")));
    let hide_others = PredefinedMenuItem::hide_others(Some(sissy::i18n::text("Ocultar os outros")));
    let show_all = PredefinedMenuItem::show_all(Some(sissy::i18n::text("Mostrar tudo")));
    let quit = PredefinedMenuItem::quit(Some(sissy::i18n::text("Encerrar Sissy")));
    let sep = || PredefinedMenuItem::separator();
    let pick = MenuItem::with_id(
        "pick",
        sissy::i18n::text("Escolher pasta…"),
        true,
        key(Code::KeyO),
    );
    let analyze = MenuItem::with_id(
        "analyze",
        sissy::i18n::text("Analisar pastas"),
        true,
        key(Code::KeyR),
    );
    let export = MenuItem::with_id(
        "export",
        sissy::i18n::text("Exportar relatório…"),
        true,
        shift(Code::KeyE),
    );
    let trash = MenuItem::with_id(
        "trash",
        sissy::i18n::text("Revisar envio ao Lixo…"),
        true,
        key(Code::Backspace),
    );
    let close = PredefinedMenuItem::close_window(Some(sissy::i18n::text("Fechar")));
    let undo = PredefinedMenuItem::undo(Some(sissy::i18n::text("Desfazer")));
    let redo = PredefinedMenuItem::redo(Some(sissy::i18n::text("Refazer")));
    let cut = PredefinedMenuItem::cut(Some(sissy::i18n::text("Recortar")));
    let copy = PredefinedMenuItem::copy(Some(sissy::i18n::text("Copiar")));
    let paste = PredefinedMenuItem::paste(Some(sissy::i18n::text("Colar")));
    let select_all = PredefinedMenuItem::select_all(Some(sissy::i18n::text("Selecionar tudo")));
    let page0 = MenuItem::with_id(
        "page-0",
        sissy::i18n::text("Modo simples"),
        true,
        key(Code::Digit1),
    );
    let page1 = MenuItem::with_id(
        "page-1",
        sissy::i18n::text("Modo avançado"),
        true,
        key(Code::Digit2),
    );
    let page2 = MenuItem::with_id(
        "page-2",
        sissy::i18n::text("Resultados"),
        true,
        key(Code::Digit3),
    );
    let page3 = MenuItem::with_id(
        "page-3",
        sissy::i18n::text("Relatórios"),
        true,
        key(Code::Digit4),
    );
    let page4 = MenuItem::with_id(
        "page-4",
        sissy::i18n::text("Dicas"),
        true,
        key(Code::Digit5),
    );
    let minimize = PredefinedMenuItem::minimize(Some(sissy::i18n::text("Minimizar")));
    let maximize = PredefinedMenuItem::maximize(Some(sissy::i18n::text("Ampliar")));
    let fullscreen = PredefinedMenuItem::fullscreen(Some(sissy::i18n::text("Tela cheia")));
    let help_tips = MenuItem::with_id("tips", sissy::i18n::text("Sem se enrolar"), true, None);
    let sep_a = sep();
    let sep_b = sep();
    let sep_c = sep();
    let sep_d = sep();
    let sep_e = sep();
    let sep_f = sep();
    let sep_g = sep();
    let Ok(app) = Submenu::with_items(
        "Sissy",
        true,
        &[
            &about_item,
            &settings,
            &sep_a,
            &services,
            &sep_b,
            &hide,
            &hide_others,
            &show_all,
            &sep_c,
            &quit,
        ],
    ) else {
        return;
    };
    let Ok(file) = Submenu::with_items(
        sissy::i18n::text("Arquivo"),
        true,
        &[&pick, &analyze, &sep_d, &export, &trash, &sep_e, &close],
    ) else {
        return;
    };
    let Ok(edit) = Submenu::with_items(
        sissy::i18n::text("Editar"),
        true,
        &[&undo, &redo, &sep_f, &cut, &copy, &paste, &select_all],
    ) else {
        return;
    };
    let Ok(view) = Submenu::with_items(
        sissy::i18n::text("Visualizar"),
        true,
        &[&page0, &page1, &page2, &page3, &page4],
    ) else {
        return;
    };
    let Ok(window) = Submenu::with_items(
        sissy::i18n::text("Janela"),
        true,
        &[&minimize, &maximize, &sep_g, &fullscreen],
    ) else {
        return;
    };
    let Ok(help) = Submenu::with_items(sissy::i18n::text("Ajuda"), true, &[&help_tips]) else {
        return;
    };
    let bar = Menu::new();
    if bar.append(&app).is_err()
        || bar.append(&file).is_err()
        || bar.append(&edit).is_err()
        || bar.append(&view).is_err()
        || bar.append(&window).is_err()
        || bar.append(&help).is_err()
    {
        return;
    }
    bar.init_for_nsapp();
    thread_local! {
        static MENU: std::cell::RefCell<Option<Box<dyn std::any::Any>>> = std::cell::RefCell::new(None);
    }
    MENU.with(|owner| {
        *owner.borrow_mut() = Some(Box::new((
            bar, app, file, edit, view, window, help, pick, analyze, export, trash, page0, page1,
            page2, page3, page4, help_tips, settings,
        )))
    });
}
fn file_row(
    ui: &mut egui::Ui,
    item: &Item,
    index: usize,
    selected: &mut HashSet<usize>,
    reveal: &mut Option<PathBuf>,
    keeper: bool,
) {
    ui.horizontal_top(|ui| {
        App::toggle(selected, ui, index);
        let width = ui.available_width();
        ui.vertical(|ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                let name_width = (width - 110.).max(40.);
                let name = fitted_name(ui, &item.path, name_width);
                ui.add_sized(
                    [name_width, 24.],
                    egui::Label::new(title(&name, 18.)).truncate(),
                )
                .on_hover_text(item.path.display().to_string());
                ui.label(title(&mib(item.size), 14.));
            });
            let folder = item
                .path
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            ui.label(
                egui::RichText::new(fitted_path(ui, &folder, width))
                    .size(12.)
                    .color(muted()),
            )
            .on_hover_text(item.path.display().to_string());
            ui.horizontal_wrapped(|ui| {
                for tag in &item.tags {
                    chip(ui, tag_name(tag), *tag == Tag::Duplicate);
                }
                if keeper {
                    chip(ui, text("Melhor para ficar"), true);
                }
                ui.label(egui::RichText::new(format_when(item.modified)).size(12.));
                if finder_button(ui).clicked() {
                    *reveal = Some(item.path.clone());
                }
            });
        });
    });
}

fn chip(ui: &mut egui::Ui, text: &str, lime: bool) {
    egui::Frame::new()
        .fill(if lime { accent() } else { soft_accent() })
        .stroke(egui::Stroke::new(1.5_f32, border()))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(title(text, 13.));
        });
}
fn tag_name(tag: &Tag) -> &'static str {
    match tag {
        Tag::Large => sissy::i18n::text("Grande"),
        Tag::OldDownload => sissy::i18n::text("Download antigo"),
        Tag::Cache => sissy::i18n::text("Cache"),
        Tag::Package => sissy::i18n::text("Mídia"),
        Tag::Duplicate => sissy::i18n::text("Cópia"),
    }
}
fn shrink_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let budget = max_chars.max(5);
    let keep = budget - 1;
    let left = keep / 2;
    let right = keep - left;
    let head: String = chars[..left].iter().collect();
    let tail: String = chars[chars.len() - right..].iter().collect();
    format!("{head}…{tail}")
}
fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), font, ink())
        .size()
        .x
}
fn fitted_name(ui: &egui::Ui, path: &std::path::Path, width: f32) -> String {
    let raw = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let font = egui::FontId::new(18., egui::FontFamily::Name("comic-bold".into()));
    let width = width.max(96.);
    if text_width(ui, &raw, font.clone()) <= width {
        return raw;
    }
    let mut low = 4usize;
    let mut high = raw.chars().count().max(4);
    let mut best = shrink_filename(&raw, low);
    while low <= high {
        let mid = (low + high) / 2;
        let candidate = shrink_filename(&raw, mid);
        if text_width(ui, &candidate, font.clone()) <= width {
            best = candidate;
            low = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    best
}
fn fitted_path(ui: &egui::Ui, text: &str, width: f32) -> String {
    let font = egui::FontId::proportional(13.);
    let width = width.max(96.);
    if text_width(ui, text, font.clone()) <= width {
        return text.to_string();
    }
    let mut low = 8usize;
    let mut high = text.chars().count().max(8);
    let mut best = shrink_middle(text, low);
    while low <= high {
        let mid = (low + high) / 2;
        let candidate = shrink_middle(text, mid);
        if text_width(ui, &candidate, font.clone()) <= width {
            best = candidate;
            low = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }
    best
}
fn mib(size: u64) -> String {
    format!("{:.1} MiB", size as f64 / 1_048_576.)
}
fn place_rank(path: &std::path::Path) -> u8 {
    let Ok(home) = std::env::var("HOME") else {
        return 1;
    };
    let Ok(rel) = path.strip_prefix(home) else {
        return 1;
    };
    match rel.components().next().and_then(|c| c.as_os_str().to_str()) {
        Some("Documents") => 0,
        Some("Desktop") => 2,
        Some("Downloads") => 3,
        _ => 1,
    }
}
fn preferred_keep(members: &[usize], items: &[Item]) -> Option<usize> {
    members.iter().copied().min_by(|&a, &b| {
        place_rank(&items[a].path)
            .cmp(&place_rank(&items[b].path))
            .then(items[a].modified.cmp(&items[b].modified))
            .then(items[a].path.cmp(&items[b].path))
    })
}
fn duplicate_groups(items: &[Item]) -> Vec<(PathBuf, Vec<usize>)> {
    let mut groups: Vec<(PathBuf, Vec<usize>)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if !item.tags.contains(&Tag::Duplicate) {
            continue;
        }
        let Some(keeper) = item.keeper.clone() else {
            continue;
        };
        if let Some((_, copies)) = groups.iter_mut().find(|(path, _)| path == &keeper) {
            copies.push(i);
        } else {
            groups.push((keeper, vec![i]));
        }
    }
    groups.sort_by(|a, b| {
        let size = |group: &(PathBuf, Vec<usize>)| {
            group
                .1
                .first()
                .and_then(|i| items.get(*i))
                .map(|item| item.size)
                .unwrap_or(0)
        };
        size(b).cmp(&size(a)).then(a.0.cmp(&b.0))
    });
    groups
}
impl App {
    fn reveal(&mut self, path: &std::path::Path) {
        #[cfg(target_os = "macos")]
        {
            let ok = std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(path)
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                self.status = sissy::i18n::text(
                    "Não foi possível mostrar no Finder. Verifique as permissões.",
                )
                .into();
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            self.status = sissy::i18n::text("Mostrar no Finder só existe no macOS.").into();
        }
    }
    fn folders(&mut self, ui: &mut egui::Ui) {
        let before = (self.large, self.days);
        let busy = self.receiver.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(title(
                    if self.advanced {
                        sissy::i18n::text("Modo avançado")
                    } else {
                        sissy::i18n::text("Modo simples")
                    },
                    22.,
                ));
                if !self.disk.is_empty() {
                    ui.label(&self.disk);
                }
            });
            ui.horizontal_wrapped(|ui| {
                let mut remove = None;
                for (i, p) in self.roots.iter().enumerate() {
                    let mut label = p.file_name().map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string());
                    if let Some(home) = std::env::var_os("HOME") {
                        if p.parent() == Some(std::path::Path::new(&home)) {
                            label = match label.as_str() {
                                "Desktop" => text("Mesa").into(), "Documents" => text("Documentos").into(),
                                "Downloads" => text("Downloads").into(), _ => label,
                            };
                        }
                    }
                    egui::Frame::new().stroke(egui::Stroke::new(1.5_f32, border()))
                        .corner_radius(8).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(shrink_middle(&label, 28)).on_hover_text(p.display().to_string());
                                if ui.small_button("×").on_hover_text(text("Tirar esta pasta")).clicked() { remove = Some(i); }
                            });
                        });
                }
                if let Some(i) = remove {
                    self.roots.remove(i);
                }
            });
            ui.horizontal_wrapped(|ui| {
                if icon_button(
                    ui,
                    egui::include_image!("../assets/icons/folder-plus-outline.svg"),
                    sissy::i18n::text("Escolher pasta…"),
                    false,
                    [190., 36.],
                    18.,
                    16.,
                )
                .clicked()
                {
                    self.pick_folder();
                }
                ui.add_enabled_ui(!self.roots.is_empty(), |ui| {
                    if icon_button(
                        ui,
                        egui::include_image!("../assets/icons/magnify.svg"),
                        sissy::i18n::text("Analisar pastas"),
                        true,
                        [190., 36.],
                        18.,
                        16.,
                    )
                    .clicked()
                    {
                        self.start();
                    }
                });
            });
            ui.label(sissy::i18n::text("HD externo: use Escolher pasta… e selecione o disco ou uma pasta dele."));
            if self.advanced {
                ui.horizontal_wrapped(|ui| {
                    ui.label(sissy::i18n::text("Grandes (MiB)"));
                    ui.add(egui::DragValue::new(&mut self.large).range(1..=1_000_000));
                    ui.label(sissy::i18n::text("Downloads antigos (dias)"));
                    ui.add(egui::DragValue::new(&mut self.days).range(1..=36_500));
                });
                ui.label(sissy::i18n::text("Para caches, escolha ~/Library/Caches. Links e locais protegidos ficam de fora."));
            }
            ui.label(
                egui::RichText::new(
                    sissy::i18n::text("A análise só lê. O Lixo só recebe o que você confirmar."),
                )
                .size(13.)
                .color(muted()),
            );
        });
        if before != (self.large, self.days) {
            self.persist_preferences();
        }
    }
    fn go(&mut self, page: usize) {
        self.page = page;
        if page < 2 {
            self.advanced = page == 1;
        }
    }
    fn pick_folder(&mut self) {
        if self.receiver.is_some() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            match std::fs::canonicalize(&path) {
                Ok(path) if !protected(&path) => {
                    if !self.roots.contains(&path) {
                        self.roots.push(path);
                    }
                }
                Ok(_) => {
                    self.status = sissy::i18n::text("Local protegido. Escolha uma pasta pessoal ou um disco em /Volumes; não selecione /Volumes inteiro.").into();
                }
                Err(error) => {
                    self.status =
                        sissy::tf!("Não foi possível acessar a pasta: {error}", error = error);
                }
            }
        }
    }
    fn poll_menu(&mut self) {
        #[cfg(target_os = "macos")]
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                "pick" => self.pick_folder(),
                "analyze" => {
                    if !self.roots.is_empty() && self.receiver.is_none() {
                        self.start();
                    }
                }
                "export" => self.log_confirm = true,
                "trash" => {
                    if !self.chosen().is_empty() && cfg!(target_os = "macos") {
                        self.confirm = true;
                    }
                }
                "page-0" => self.go(0),
                "page-1" => self.go(1),
                "page-2" => self.go(2),
                "page-3" => self.go(3),
                "page-4" | "tips" => self.go(4),
                "settings" => self.go(5),
                _ => {}
            }
        }
    }
    fn toggle(selected: &mut HashSet<usize>, ui: &mut egui::Ui, index: usize) {
        let mut on = selected.contains(&index);
        ui.checkbox(&mut on, "");
        if on {
            selected.insert(index);
        } else {
            selected.remove(&index);
        }
    }
    fn space_ranking(&mut self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            return;
        };
        let mut reveal = None;
        egui::CollapsingHeader::new(sissy::i18n::text("Top 10 de espaço"))
            .default_open(true).show(ui, |ui| {
                ui.label(sissy::i18n::text("Somente pastas escolhidas. Tamanho lógico; pastas incluem subpastas e podem se sobrepor. Não some as linhas."));
                if report.cancelled || !report.errors.is_empty() {
                    ui.colored_label(muted(), sissy::i18n::text("Ranking parcial: análise cancelada ou com avisos. Locais protegidos não entram na contagem."));
                }
                ui.horizontal_wrapped(|ui| {
                    for (value, label) in [(0, "Pastas e arquivos"), (1, "Pastas"), (2, "Arquivos")] {
                        ui.selectable_value(&mut self.space_filter, value, sissy::i18n::text(label));
                    }
                });
                let mut entries: Vec<_> = report.top_folders.iter().chain(&report.top_files)
                    .filter(|entry| match self.space_filter { 1 => entry.is_dir, 2 => !entry.is_dir, _ => true })
                    .collect();
                entries.sort_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
                if entries.is_empty() { ui.label(sissy::i18n::text("Nenhum item analisado nesta categoria.")); }
                for (rank, entry) in entries.into_iter().take(10).enumerate() {
                    ui.push_id(("space", rank), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(format!("{}. {}", rank + 1, mib(entry.size)));
                            ui.label(sissy::i18n::text(if entry.is_dir { "Pasta" } else { "Arquivo" }));
                            if finder_button(ui).clicked() { reveal = Some(entry.path.clone()); }
                        });
                        let name = fitted_path(ui, &entry.path.display().to_string(), ui.available_width());
                        ui.label(name).on_hover_text(entry.path.display().to_string());
                        ui.separator();
                    });
                }
            });
        if let Some(path) = reveal {
            self.reveal(&path);
        }
    }

    fn results(&mut self, ui: &mut egui::Ui, card_h: f32) {
        let count = self.report.as_ref().map_or(0, |r| r.items.len());
        let heavy: u64 = self
            .report
            .as_ref()
            .map_or(0, |r| r.items.iter().map(|i| i.size).sum());
        let top = ui.cursor().min.y;
        ui.horizontal(|ui| {
            ui.label(title(sissy::i18n::text("Para revisar"), 22.));
            ui.label(title(&format!("{count}"), 16.).background_color(accent()));
            ui.label(mib(heavy));
        });
        let mut reveal: Option<PathBuf> = None;
        if self.report.is_some() {
            let visited = self.report.as_ref().map(|r| r.visited).unwrap_or(0);
            let warnings = self.report.as_ref().map(|r| r.errors.len()).unwrap_or(0);
            let cancelled = self.report.as_ref().is_some_and(|r| r.cancelled);
            ui.label(
                egui::RichText::new(sissy::tf!(
                    "{visited} examinados • {warnings} avisos{}",
                    if cancelled {
                        sissy::i18n::text(" • resultado parcial")
                    } else {
                        ""
                    },
                    visited = visited,
                    warnings = warnings
                ))
                .size(13.)
                .color(muted()),
            );
            let filters = [
                (0u8, sissy::i18n::text("Todos"), count),
                (
                    1,
                    sissy::i18n::text("Grandes"),
                    self.report
                        .as_ref()
                        .map(|r| {
                            r.items
                                .iter()
                                .filter(|i| i.tags.contains(&Tag::Large))
                                .count()
                        })
                        .unwrap_or(0),
                ),
                (
                    2,
                    sissy::i18n::text("Duplicados"),
                    self.report
                        .as_ref()
                        .map(|r| {
                            r.items
                                .iter()
                                .filter(|i| i.tags.contains(&Tag::Duplicate))
                                .count()
                        })
                        .unwrap_or(0),
                ),
                (
                    3,
                    text("Downloads"),
                    self.report
                        .as_ref()
                        .map(|r| {
                            r.items
                                .iter()
                                .filter(|i| i.tags.contains(&Tag::OldDownload))
                                .count()
                        })
                        .unwrap_or(0),
                ),
                (
                    4,
                    sissy::i18n::text("Caches"),
                    self.report
                        .as_ref()
                        .map(|r| {
                            r.items
                                .iter()
                                .filter(|i| i.tags.contains(&Tag::Cache))
                                .count()
                        })
                        .unwrap_or(0),
                ),
                (
                    5,
                    sissy::i18n::text("Mídia"),
                    self.report
                        .as_ref()
                        .map(|r| {
                            r.items
                                .iter()
                                .filter(|i| i.tags.contains(&Tag::Package))
                                .count()
                        })
                        .unwrap_or(0),
                ),
            ];
            ui.horizontal_wrapped(|ui| {
                for (id, label, n) in filters {
                    if icon_button(
                        ui,
                        filter_icon(id),
                        &format!("{label} {n}"),
                        self.focus == id,
                        [156., 34.],
                        16.,
                        15.,
                    )
                    .clicked()
                    {
                        self.focus = id;
                    }
                }
            });
            let visible: Vec<usize> = self
                .report
                .as_ref()
                .map(|r| {
                    r.items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| matches_focus(item, self.focus))
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default();
            ui.horizontal(|ui| {
                if icon_button(
                    ui,
                    egui::include_image!("../assets/icons/checkbox-multiple-marked-outline.svg"),
                    sissy::i18n::text("Selecionar visíveis"),
                    false,
                    [210., 34.],
                    16.,
                    15.,
                )
                .clicked()
                {
                    for i in &visible {
                        self.selected.insert(*i);
                    }
                }
                if icon_button(
                    ui,
                    egui::include_image!("../assets/icons/close-circle-outline.svg"),
                    sissy::i18n::text("Limpar seleção"),
                    false,
                    [180., 34.],
                    16.,
                    15.,
                )
                .clicked()
                {
                    self.selected.clear();
                }
            });
            if self.focus == 2 {
                ui.label(
                    egui::RichText::new(
                        sissy::i18n::text("Marque só o que pode ir ao Lixo. Pode manter Mesa e Documentos e apagar só Downloads."),
                    )
                    .size(13.),
                );
            }
            let used = ui.cursor().min.y - top;
            let list_h = (card_h - used - 58.).clamp(130., 560.);
            egui::ScrollArea::vertical()
                .id_salt("results")
                .max_height(list_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.focus == 2 {
                        self.duplicate_list(ui, &mut reveal);
                    } else {
                        self.flat_list(ui, &visible, &mut reveal);
                    }
                });
        } else {
            let used = ui.cursor().min.y - top;
            let list_h = (card_h - used - 58.).clamp(130., 560.);
            ui.allocate_ui(egui::vec2(ui.available_width(), list_h.min(95.)), |ui| {
                ui.add_space(12.);
                ui.label(sissy::i18n::text(
                    "Analise uma pasta para ver os arquivos aqui.",
                ));
                ui.label(sissy::i18n::text(
                    "Nomes longos mantêm o tipo no fim, por exemplo ferias... .mp4.",
                ));
                ui.label(sissy::i18n::text(
                    "Nada vem selecionado. O maior aparece primeiro.",
                ));
            });
        }
        if let Some(path) = reveal {
            self.reveal(&path);
        }
        let chosen = self.chosen();
        let bytes: u64 = chosen.iter().map(|i| i.size).sum();
        ui.separator();
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(title(
                &sissy::tf!("Selecionados: {}  •  {}", chosen.len(), mib(bytes)),
                16.,
            ));
            let wipes_a_group = self.report.as_ref().is_some_and(|report| {
                duplicate_groups(&report.items).iter().any(|(_, members)| {
                    !members.is_empty() && members.iter().all(|i| self.selected.contains(i))
                })
            });
            if wipes_a_group {
                ui.label(sissy::i18n::text(
                    "Um grupo está inteiro marcado. Esse arquivo sai de todos os lugares.",
                ));
            }
            ui.add_enabled_ui(!chosen.is_empty() && cfg!(target_os = "macos"), |ui| {
                if icon_button(
                    ui,
                    egui::include_image!("../assets/icons/trash-can-outline.svg"),
                    sissy::i18n::text("Revisar envio ao Lixo…"),
                    true,
                    [250., 36.],
                    18.,
                    16.,
                )
                .clicked()
                {
                    self.confirm = true;
                }
            });
            if icon_button(
                ui,
                egui::include_image!("../assets/icons/file-export-outline.svg"),
                sissy::i18n::text("Exportar relatório…"),
                false,
                [220., 36.],
                18.,
                16.,
            )
            .clicked()
            {
                self.log_confirm = true;
            }
        });
    }
    fn flat_list(&mut self, ui: &mut egui::Ui, visible: &[usize], reveal: &mut Option<PathBuf>) {
        let Some(report) = &self.report else {
            return;
        };
        if visible.is_empty() {
            if report.visited == 0 {
                ui.label(sissy::i18n::text("Nenhum arquivo foi examinado. Verifique a pasta escolhida, a conexão do disco e os avisos abaixo."));
            } else if report.items.is_empty() {
                ui.label(sissy::i18n::text("Análise concluída: nenhum arquivo atende aos critérios atuais. No modo Avançado, você pode ajustar o limite de tamanho."));
            } else {
                ui.label(sissy::i18n::text("Nenhum resultado neste filtro. Selecione Todos para ver os outros arquivos encontrados."));
            }
            for error in &report.errors {
                ui.label(error);
            }
            return;
        }
        for &index in visible {
            let Some(item) = report.items.get(index) else {
                continue;
            };
            ui.add_space(4.);
            egui::Frame::new()
                .stroke(egui::Stroke::new(1.5_f32, border()))
                .corner_radius(10)
                .inner_margin(8)
                .show(ui, |ui| {
                    file_row(ui, item, index, &mut self.selected, reveal, false);
                });
        }
        for error in &report.errors {
            ui.label(error);
        }
    }
    fn duplicate_list(&mut self, ui: &mut egui::Ui, reveal: &mut Option<PathBuf>) {
        let Some(report) = &self.report else {
            return;
        };
        let groups = duplicate_groups(&report.items);
        if groups.is_empty() {
            ui.label(sissy::i18n::text(
                "Nenhuma duplicata de 1 MiB ou mais nesta análise.",
            ));
            return;
        }
        for (_, copies) in &groups {
            let mut members = copies.clone();
            members.sort_by(|&a, &b| {
                place_rank(&report.items[a].path)
                    .cmp(&place_rank(&report.items[b].path))
                    .then(report.items[a].path.cmp(&report.items[b].path))
            });
            let keep = preferred_keep(&members, &report.items);
            let size = members
                .first()
                .and_then(|i| report.items.get(*i))
                .map(|item| item.size)
                .unwrap_or(0);
            let places = members.len();
            let all_marked = members.iter().all(|i| self.selected.contains(i));
            ui.add_space(6.);
            egui::Frame::new()
                .fill(soft_accent())
                .stroke(egui::Stroke::new(2_f32, border()))
                .corner_radius(12)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(title(
                            &sissy::tf!("Mesmo arquivo em {places} lugares", places = places),
                            18.,
                        ));
                        ui.label(sissy::tf!("{} cada", mib(size)));
                        if icon_button(
                            ui,
                            egui::include_image!("../assets/icons/broom.svg"),
                            sissy::i18n::text("Sugerir o que soltar"),
                            false,
                            [230., 32.],
                            16.,
                            15.,
                        )
                        .clicked()
                        {
                            for index in &members {
                                if Some(*index) == keep {
                                    self.selected.remove(index);
                                } else {
                                    self.selected.insert(*index);
                                }
                            }
                        }
                    });
                    ui.label(
                        egui::RichText::new(sissy::i18n::text(
                            "Marque as cópias que podem ir ao Lixo. As desmarcadas ficam.",
                        ))
                        .size(12.)
                        .color(muted()),
                    );
                    if all_marked {
                        ui.label(sissy::i18n::text("Este grupo sairia de todos os lugares."));
                    }
                    for index in &members {
                        let Some(item) = report.items.get(*index) else {
                            continue;
                        };
                        ui.separator();
                        file_row(
                            ui,
                            item,
                            *index,
                            &mut self.selected,
                            reveal,
                            Some(*index) == keep,
                        );
                    }
                });
        }
        for error in &report.errors {
            ui.label(error);
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        #[cfg(target_os = "macos")]
        {
            refresh_icon_appearance();
            ctx.request_repaint_after(std::time::Duration::from_millis(400));
        }
        self.poll_menu();
        apply_theme(ctx, self.preferences.theme);
        ctx.set_zoom_factor(self.preferences.scale);
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok(r) => {
                    self.report = Some(r);
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.status = sissy::i18n::text(
                        "A análise foi interrompida inesperadamente. Tente novamente.",
                    )
                    .into();
                }
                _ => {}
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let compact = ctx.screen_rect().width() < 960.;
        egui::TopBottomPanel::top("brand")
            .frame(
                egui::Frame::new()
                    .fill(accent())
                    .inner_margin(12)
                    .stroke(egui::Stroke::new(2_f32, border())),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(icon) = &self.mascot {
                        ui.image((icon.id(), egui::vec2(44., 44.)));
                    }
                    ui.label(title("SISSY", 30.));
                    if !compact {
                        ui.separator();
                        ui.label(text("Espaço para escolher\ne revisar arquivos"));
                    }
                });
            });
        egui::TopBottomPanel::bottom("safety")
            .frame(
                egui::Frame::new()
                    .fill(accent())
                    .inner_margin(10)
                    .stroke(egui::Stroke::new(2_f32, border())),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(title(
                        sissy::i18n::text(
                            "Análise local. Nenhum arquivo é removido durante a análise.",
                        ),
                        15.,
                    ));
                });
            });
        let labels = [
            text("Iniciante"),
            text("Avançado"),
            text("Resultados"),
            text("Relatórios"),
            text("Dicas"),
            text("Ajustes"),
        ];
        if compact {
            egui::TopBottomPanel::top("navigation-tabs").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (n, label) in labels.iter().enumerate() {
                        if ui.selectable_label(self.page == n, *label).clicked() {
                            self.go(n);
                        }
                    }
                });
            });
        } else {
            egui::SidePanel::left("navigation")
                .exact_width(172.)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(surface())
                        .inner_margin(10)
                        .stroke(egui::Stroke::new(2_f32, border())),
                )
                .show(ctx, |ui| {
                    for (n, label) in labels.iter().enumerate() {
                        if icon_button(
                            ui,
                            nav_icon(n),
                            label,
                            self.page == n,
                            [150., 38.],
                            18.,
                            14.,
                        )
                        .clicked()
                        {
                            self.go(n);
                        }
                        ui.add_space(6.);
                    }
                    ui.separator();
                    ui.label(text("Sem nuvem.\nSem pressa.\nVocê no controle."));
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(canvas())
                    .inner_margin(16),
            )
            .show(ctx, |ui| {
                let viewport_height = ui.available_height();
                egui::ScrollArea::vertical().id_salt(("workspace", self.page)).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if self.page <= 2 {
                    if self.page < 2 {
                        card().show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            self.folders(ui);
                        });
                        ui.add_space(10.);
                    }
                    if self.receiver.is_some() {
                        card().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(title(sissy::i18n::text("Análise em andamento…"), 20.));
                            });
                            ui.label(sissy::tf!("{} arquivos examinados",
                                self.progress.load(Ordering::Relaxed)));
                            if icon_button(
                                ui,
                                egui::include_image!("../assets/icons/close-circle-outline.svg"),
                                sissy::i18n::text("Cancelar análise"),
                                false,
                                [210., 34.],
                                16.,
                                15.,
                            )
                            .clicked()
                            {
                                self.cancel.store(true, Ordering::Relaxed);
                            }
                        });
                        ui.add_space(10.);
                    }
                    let budget = (viewport_height - if self.page < 2 { 240. } else { 0. }).max(260.);
                    card().show(ui, |ui| {
                        ui.set_width(ui.available_width());

                        self.space_ranking(ui);
                        ui.add_space(10.);
                        self.results(ui, budget);
                    });
                } else {
                    egui::ScrollArea::vertical().id_salt(("main", self.page)).show(ui, |ui| {
                        match self.page {
                            3 => {
                                card().show(ui, |ui| {
                                    ui.label(title(sissy::i18n::text("Relatórios locais"), 28.));
                                    ui.label(sissy::i18n::text("Exportação opcional. Sem nomes ou caminhos por padrão; você revisará o conteúdo antes de salvar."));
                                    if icon_button(
                                        ui,
                                        egui::include_image!("../assets/icons/file-export-outline.svg"),
                                        sissy::i18n::text("Criar relatório…"),
                                        true,
                                        [230., 44.],
                                        18.,
                                        16.,
                                    )
                                    .clicked()
                                    {
                                        self.log_confirm = true;
                                    }
                                    for (i, outcome) in self.outcomes.iter().enumerate() {
                                        ui.label(outcome);
                                        if let Some(p) = self.action_paths.get(i) {
                                            ui.label(p.display().to_string());
                                        }
                                    }
                                });
                            }
                            4 => tips(ui),
                            5 => self.settings(ui, ctx),
                            _ => {}
                        }
                        if !self.status.is_empty() {
                            ui.add_space(10.);
                            ui.label(&self.status);
                        }
                    });
                }
                });
            });
        if self.confirm {
            egui::Window::new(sissy::i18n::text("Confirmar envio ao Lixo")).collapsible(false).resizable(true).default_width(560.).show(ctx,|ui|{
                let items=self.chosen();
                ui.label(sissy::tf!("Mover {} arquivos ({:.2} MiB) ao Lixo? Não há exclusão permanente. Esvazie o Lixo no Finder só depois de conferir.",items.len(),items.iter().map(|i|i.size).sum::<u64>() as f64/1048576.));
                egui::ScrollArea::vertical().id_salt("confirm-paths").max_height(240.).show(ui,|ui|{
                    for item in &items {
                        ui.label(format!("{:.2} MiB  {}", item.size as f64/1048576., item.path.display()));
                    }
                });
                ui.label(sissy::i18n::text("O macOS pode pedir permissão para o Sissy controlar o Finder."));
                if ui.button(sissy::i18n::text("Confirmo: mover ao Lixo")).clicked(){let results=act(&items,true,trash);self.action_paths=results.iter().map(|(p,_)|p.clone()).collect();self.outcomes=results.iter().enumerate().map(|(n,(_,r))|sissy::tf!("Item {}: {}",n+1,match r{Ok(_)=>sissy::i18n::text("Movido ao Lixo").into(),Err(e)=>sissy::tf!("Falhou: {e}", e = e)})).collect();self.status=sissy::i18n::text("Ações concluídas. Analise novamente antes de outra operação.").into();self.selected.clear();self.report=None;self.confirm=false;self.page=3;}
                if ui.button(sissy::i18n::text("Cancelar operação")).clicked(){self.confirm=false;}
            });
        }
        if self.log_confirm {
            egui::Window::new(sissy::i18n::text("Log opcional")).collapsible(false).show(ctx,|ui|{ui.label(sissy::i18n::text("Será salvo apenas um resumo da análise e resultados das ações. Sem nomes ou caminhos por padrão. O arquivo fica no destino escolhido; nenhuma transmissão é feita."));ui.checkbox(&mut self.paths_in_log,sissy::i18n::text("Incluir explicitamente os caminhos dos resultados (dados sensíveis)"));if ui.button(sissy::i18n::text("Criar log local…")).clicked(){if let Some(p)=rfd::FileDialog::new().set_file_name("sissy-log.txt").save_file(){let mut text=if let Some(r)=&self.report{sissy::tf!("Examinados: {}; candidatos: {}; cancelada: {}; avisos: {}\n",r.visited,r.items.len(),r.cancelled,r.errors.len())}else{sissy::i18n::text("Sem análise atual\n").into()};text.push_str(&self.outcomes.join("\n"));if self.paths_in_log{for p in &self.action_paths{text.push_str(&sissy::tf!("\nAção: {}",p.display()));}if let Some(r)=&self.report{for i in &r.items{text.push_str(&format!("\n{}",i.path.display()));}}}self.status=match std::fs::write(p,text){Ok(_)=>sissy::i18n::text("Log salvo localmente").into(),Err(e)=>sissy::tf!("Falha ao salvar log: {e}", e = e)};}self.log_confirm=false;}if ui.button(sissy::i18n::text("Cancelar log")).clicked(){self.log_confirm=false;}});
        }
    }
}
fn tip_card(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    headline: &str,
    body: &str,
    lime: bool,
) {
    egui::Frame::new()
        .fill(if lime { accent() } else { surface() })
        .stroke(egui::Stroke::new(2_f32, border()))
        .corner_radius(14)
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.;
                egui::Frame::new()
                    .fill(if lime { surface() } else { accent() })
                    .stroke(egui::Stroke::new(2_f32, border()))
                    .corner_radius(12)
                    .inner_margin(8)
                    .show(ui, |ui| {
                        ui.add(
                            themed_image(icon)
                                .fit_to_exact_size(egui::vec2(34., 34.))
                                .alt_text(headline),
                        );
                    });
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(title(headline, 20.));
                    ui.add(egui::Label::new(egui::RichText::new(body).size(15.)).wrap());
                });
            });
        });
    ui.add_space(10.);
}
fn tips(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.;
        egui::Frame::new()
            .fill(surface())
            .stroke(egui::Stroke::new(2_f32, border()))
            .corner_radius(16)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.add(
                    themed_image(egui::include_image!("../assets/icons/snake.svg"))
                        .fit_to_exact_size(egui::vec2(42., 42.))
                        .alt_text(sissy::i18n::text("Cobra")),
                );
            });
        ui.vertical(|ui| {
            ui.label(title(sissy::i18n::text("Sem se enrolar"), 28.));
            ui.label(sissy::i18n::text(
                "A cobra aponta. O bote continua sendo seu.",
            ));
        });
    });
    ui.add_space(12.);
    tip_card(
        ui,
        egui::include_image!("../assets/icons/target.svg"),
        sissy::i18n::text("Não dê o bote à toa"),
        sissy::i18n::text("Um filme enorme na pasta certa deve ficar. O Sissy só ergue a cabeça para tamanho, download antigo, cache ou mídia. O bote, se houver, é você quem dá."),
        true,
    );
    tip_card(
        ui,
        egui::include_image!("../assets/icons/file-search-outline.svg"),
        sissy::i18n::text("A língua marca o tipo"),
        sissy::i18n::text("Nome comprido aparece como ferias... .mp4. A língua para no tipo do arquivo e não come o .mp4, .zip ou .dmg. A linha de baixo é a pasta. Passe o mouse para ver o caminho inteiro."),
        false,
    );
    tip_card(
        ui,
        egui::include_image!("../assets/icons/content-copy.svg"),
        sissy::i18n::text("Mais de um ninho"),
        sissy::i18n::text("A mesma foto pode estar na Mesa, nos Downloads e em Documentos. Marque só o ninho que pode ir embora. Pode ficar com dois e soltar o terceiro. “Sugerir o que soltar” aponta as cópias mais fáceis, em geral Downloads, e deixa Documentos quieto."),
        true,
    );
    tip_card(
        ui,
        egui::include_image!("../assets/icons/power-plug-off-outline.svg"),
        sissy::i18n::text("Não se enrosque no cache"),
        sissy::i18n::text("Cache de programa aberto volta, ou o programa chia na próxima abertura. Feche o dono daquele cache, olhe de novo, e só então confirme."),
        false,
    );
    tip_card(
        ui,
        egui::include_image!("../assets/icons/trash-can-outline.svg"),
        sissy::i18n::text("O Lixo não é a toca"),
        sissy::i18n::text("Nada é engolido de verdade. A confirmação lista cada arquivo. Abra o Lixo no Finder, confira a presa, e só então esvazie."),
        true,
    );
    tip_card(
        ui,
        egui::include_image!("../assets/icons/lock-outline.svg"),
        sissy::i18n::text("Toca fechada"),
        sissy::i18n::text("Pasta vazia quase sempre é o macOS sem abrir a toca. Ele pede acesso a Documentos, Mesa e Downloads. É só leitura, neste Mac, sem nuvem."),
        false,
    );
    ui.add_space(4.);
    ui.label(
        egui::RichText::new(sissy::i18n::text(
            "Ícones Material Design, licença Apache 2.0.",
        ))
        .size(12.)
        .color(muted()),
    );
}
fn main() -> eframe::Result {
    let preferences = Preferences::load();
    set_language(preferences.language);
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let roots = ["Downloads", "Documents", "Desktop"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect();
    let image = image::load_from_memory(include_bytes!("../assets/Sissy-composed.png"))
        .expect("Ícone PNG válido")
        .into_rgba8();
    let dimensions = [image.width() as usize, image.height() as usize];
    let icon = egui::IconData {
        rgba: image.as_raw().clone(),
        width: image.width(),
        height: image.height(),
    };
    eframe::run_native(
        "Sissy",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1100., 760.])
                .with_min_inner_size([560., 480.])
                .with_icon(icon),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            for (name, bytes) in [
                (
                    "comic",
                    include_bytes!("../assets/fonts/Comic Sans MS.ttf").as_slice(),
                ),
                (
                    "comic-bold",
                    include_bytes!("../assets/fonts/Comic Sans MS Bold.ttf").as_slice(),
                ),
            ] {
                fonts.font_data.insert(
                    name.into(),
                    egui::FontData::from_owned(bytes.to_vec()).into(),
                );
            }
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .insert(0, "comic".into());
            fonts.families.insert(
                egui::FontFamily::Name("comic-bold".into()),
                vec!["comic-bold".into()],
            );
            #[cfg(target_os = "macos")]
            if let Ok(entries) = std::fs::read_dir("/System/Library/Fonts") {
                if let Some(bytes) = entries.flatten().find_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.contains('角') && name.ends_with("W3.ttc") {
                        std::fs::read(entry.path()).ok()
                    } else {
                        None
                    }
                }) {
                    fonts
                        .font_data
                        .insert("japanese".into(), egui::FontData::from_owned(bytes).into());
                    for family in [
                        egui::FontFamily::Proportional,
                        egui::FontFamily::Name("comic-bold".into()),
                    ] {
                        fonts
                            .families
                            .entry(family)
                            .or_default()
                            .push("japanese".into());
                    }
                }
            }
            egui_extras::install_image_loaders(&cc.egui_ctx);
            #[cfg(target_os = "macos")]
            install_mac_menu();
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.all_styles_mut(|style| {
                style.spacing.item_spacing = egui::vec2(10., 8.);
                style.spacing.button_padding = egui::vec2(12., 8.);
                style
                    .text_styles
                    .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
                style
                    .text_styles
                    .insert(egui::TextStyle::Small, egui::FontId::proportional(13.));
            });
            apply_theme(&cc.egui_ctx, preferences.theme);
            cc.egui_ctx.set_zoom_factor(preferences.scale);
            #[cfg(target_os = "macos")]
            install_icon_appearances();
            let mascot = cc.egui_ctx.load_texture(
                "Sissy",
                egui::ColorImage::from_rgba_unmultiplied(dimensions, image.as_raw()),
                Default::default(),
            );
            Ok(Box::new(App {
                mascot: Some(mascot),
                roots,
                large: preferences.large,
                days: preferences.days,
                preferences,
                disk: volume_line(),
                ..Default::default()
            }))
        }),
    )
}
