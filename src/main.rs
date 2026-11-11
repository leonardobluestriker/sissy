use eframe::egui;
use sissy::*;
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
}
impl App {
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
const LIME: egui::Color32 = egui::Color32::from_rgb(204, 255, 0);
fn title(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .family(egui::FontFamily::Name("comic-bold".into()))
}
fn card() -> egui::Frame {
    egui::Frame::new()
        .shadow(egui::epaint::Shadow {
            offset: [4, 4],
            blur: 0,
            spread: 0,
            color: egui::Color32::BLACK,
        })
        .fill(egui::Color32::WHITE)
        .stroke(egui::Stroke::new(2_f32, egui::Color32::BLACK))
        .corner_radius(12)
        .inner_margin(18)
}
fn button(ui: &mut egui::Ui, text: &str, green: bool, size: [f32; 2]) -> egui::Response {
    ui.add_sized(
        size,
        egui::Button::new(title(text, 18.))
            .fill(if green { LIME } else { egui::Color32::WHITE })
            .stroke(egui::Stroke::new(2_f32, egui::Color32::BLACK))
            .corner_radius(10),
    )
}
impl App {
    fn folders(&mut self, ui: &mut egui::Ui) {
        let busy = self.receiver.is_some();
        ui.add_enabled_ui(!busy,|ui| {
   ui.horizontal(|ui|{ui.label(title(if self.advanced {"Modo avançado"} else {"Modo simples"},28.));});
   ui.label("Escolha pastas e receba sugestões de itens para revisar.");
   egui::Frame::new().fill(LIME).corner_radius(12).inner_margin(6).show(ui,|ui|{ui.label(title("As sugestões exigem revisão humana. Você decide.",16.));});
   ui.add_space(12.);
   let mut remove=None;
   for (i,p) in self.roots.iter().enumerate() {ui.horizontal(|ui|{egui::Frame::new().stroke(egui::Stroke::new(2_f32,egui::Color32::BLACK)).corner_radius(6).inner_margin(6).show(ui,|ui|{ui.set_min_width(420.);ui.label(p.display().to_string());});if ui.small_button("Remover").clicked(){remove=Some(i);}});}
   if let Some(i)=remove {self.roots.remove(i);}
   if button(ui,"Escolher pasta…",false,[190.,38.]).clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){if !self.roots.contains(&p){self.roots.push(p);}}}
   if self.advanced {ui.add_space(10.);ui.horizontal(|ui|{ui.label("Grandes (MiB)");ui.add(egui::DragValue::new(&mut self.large).range(1..=1000000));ui.label("Downloads antigos (dias)");ui.add(egui::DragValue::new(&mut self.days).range(1..=36500));});ui.label("Links simbólicos e locais protegidos são ignorados. Para caches, selecione ~/Library/Caches explicitamente.");}
   ui.add_space(12.);
   ui.add_enabled_ui(!self.roots.is_empty(),|ui|{if button(ui,"Analisar pastas",true,[ui.available_width(),48.]).clicked(){self.start();}});
  });
    }
    fn results(&mut self, ui: &mut egui::Ui) {
        let count = self.report.as_ref().map_or(0, |r| r.items.len());
        ui.horizontal(|ui| {
            ui.label(title("Itens sugeridos para revisão", 24.));
            ui.label(title(&format!("  {count} itens"), 17.).background_color(LIME));
        });
        ui.add_space(8.);
        if let Some(r) = &self.report {
            ui.label(format!(
                "{} arquivos examinados • {} avisos{}",
                r.visited,
                r.errors.len(),
                if r.cancelled {
                    " • Análise cancelada: resultados parciais"
                } else {
                    ""
                }
            ));
            egui::ScrollArea::vertical().id_salt("results").max_height(260.).show(ui,|ui|{
    for (i,item) in r.items.iter().enumerate(){
     ui.separator();
     ui.horizontal(|ui|{let mut selected=self.selected.contains(&i);ui.checkbox(&mut selected,"");if selected {self.selected.insert(i);}else{self.selected.remove(&i);}
      ui.vertical(|ui|{ui.label(title(&item.path.file_name().unwrap_or_default().to_string_lossy(),18.));ui.label(item.path.display().to_string());});
      ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(title(&format!("{:.2} MiB",item.size as f64/1048576.),17.));});
     });
     egui::CollapsingHeader::new("Motivos, data e ações").id_salt(i).show(ui,|ui|{
      for reason in &item.reasons{ui.label(reason);}
      if let Some(k)=&item.keeper{ui.label(title("Cópia a preservar",16.));ui.label(k.display().to_string());}
      ui.label(format!("Modificação: {} segundos após Unix epoch",item.modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
      ui.horizontal(|ui|{for (label,reveal) in [("Abrir arquivo",false),("Mostrar no Finder",true)]{
       if ui.add_enabled(cfg!(target_os="macos"),egui::Button::new(label)).clicked(){
        #[cfg(target_os="macos")] {let mut cmd=std::process::Command::new("/usr/bin/open");if reveal{cmd.arg("-R");}if !cmd.arg(&item.path).status().is_ok_and(|s|s.success()){self.status="Não foi possível abrir; verifique permissões.".into();}}
        #[cfg(not(target_os="macos"))] let _=reveal;
       }
      }});
     });
    }
    if self.advanced{for e in &r.errors{ui.label(e);}}
   });
        } else {
            ui.add_space(18.);
            ui.label("Sua próxima escolha começa com uma análise.");
            ui.label("Os resultados aparecerão aqui. Nenhum item vem selecionado.");
            ui.add_space(18.);
        }
        let chosen = self.chosen();
        let bytes: u64 = chosen.iter().map(|i| i.size).sum();
        ui.separator();
        ui.label(title(
            &format!(
                "Selecionados: {}  •  {:.2} MiB",
                chosen.len(),
                bytes as f64 / 1048576.
            ),
            17.,
        ));
        ui.label("Tamanho lógico estimado. O espaço recuperado pode variar.");
        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!chosen.is_empty() && cfg!(target_os = "macos"), |ui| {
                if button(ui, "Revisar envio ao Lixo…", true, [260., 42.]).clicked() {
                    self.confirm = true;
                }
            });
            if button(ui, "Exportar relatório…", false, [220., 42.]).clicked() {
                self.log_confirm = true;
            }
        });
        if !cfg!(target_os = "macos") {
            ui.small("Nesta versão Linux, Finder e Lixo não estão disponíveis.");
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(rx) = &self.receiver {
            match rx.try_recv() {
                Ok(r) => {
                    self.report = Some(r);
                    self.receiver = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.receiver = None;
                    self.status =
                        "A análise foi interrompida inesperadamente. Tente novamente.".into();
                }
                _ => {}
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::TopBottomPanel::top("brand")
            .frame(
                egui::Frame::new()
                    .fill(LIME)
                    .inner_margin(18)
                    .stroke(egui::Stroke::new(3_f32, egui::Color32::BLACK)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(icon) = &self.mascot {
                        ui.image((icon.id(), egui::vec2(85., 85.)));
                    }
                    egui::Frame::new()
                        .fill(egui::Color32::WHITE)
                        .stroke(egui::Stroke::new(3_f32, egui::Color32::BLACK))
                        .corner_radius(28)
                        .inner_margin(egui::Margin::symmetric(20, 0))
                        .show(ui, |ui| {
                            ui.label(title("SISSY", 42.));
                        });
                    ui.add_space(18.);
                    ui.separator();
                    ui.add_space(18.);
                    ui.label(title("Espaço para escolher\ne revisar arquivos", 23.));
                });
            });
        egui::TopBottomPanel::bottom("safety")
            .frame(
                egui::Frame::new()
                    .fill(LIME)
                    .inner_margin(10)
                    .stroke(egui::Stroke::new(2_f32, egui::Color32::BLACK)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(title(
                        "Análise local. Nenhum arquivo é removido durante a análise.",
                        15.,
                    ));
                });
            });
        egui::SidePanel::left("navigation")
            .exact_width(210.)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(14)
                    .stroke(egui::Stroke::new(2_f32, egui::Color32::BLACK)),
            )
            .show(ctx, |ui| {
                ui.add_space(10.);
                for (n, label) in ["Iniciante", "Avançado", "Resultados", "Relatórios", "Dicas"]
                    .iter()
                    .enumerate()
                {
                    if button(ui, label, self.page == n, [180., 52.]).clicked() {
                        self.page = n;
                        if n < 2 {
                            self.advanced = n == 1;
                        }
                    }
                    ui.add_space(10.);
                }
                ui.separator();
                ui.label("Sem nuvem.\nSem pressa.\nVocê no controle.");
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if let Some(icon) = &self.mascot {
                        ui.image((icon.id(), egui::vec2(165., 165.)));
                    }
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(egui::Color32::from_gray(246)).inner_margin(20)).show(ctx,|ui|{
   egui::ScrollArea::vertical().id_salt("main").show(ui,|ui|{
    match self.page {
     0|1=>{card().show(ui,|ui|{ui.set_width(ui.available_width());self.folders(ui);});ui.add_space(16.);},
     3=>{card().show(ui,|ui|{ui.label(title("Relatórios locais",28.));ui.label("Exportação opcional. Sem nomes ou caminhos por padrão; você revisará o conteúdo antes de salvar.");if button(ui,"Criar relatório…",true,[230.,44.]).clicked(){self.log_confirm=true;}for (i,outcome) in self.outcomes.iter().enumerate(){ui.label(outcome);if let Some(p)=self.action_paths.get(i){ui.label(p.display().to_string());}}});},
     4=>{card().show(ui,|ui|{ui.label(title("Um pouco de cuidado vai longe",28.));ui.label("1. Escolha apenas as pastas que deseja analisar.\n2. Revise o motivo e o caminho de cada sugestão.\n3. Em duplicados, confira a cópia a preservar.\n4. Selecione com calma; mover ao Lixo exige confirmação.");ui.add_space(14.);ui.label("Grande, antigo ou em cache não significa dispensável. Feche aplicativos antes de revisar seus caches. Permissões do macOS são respeitadas.");});},_=>{}
    }
    if self.receiver.is_some(){card().show(ui,|ui|{ui.horizontal(|ui|{ui.spinner();ui.label(title("Análise em andamento…",20.));});ui.label(format!("{} arquivos examinados • total ainda desconhecido",self.progress.load(Ordering::Relaxed)));if button(ui,"Cancelar análise",false,[210.,38.]).clicked(){self.cancel.store(true,Ordering::Relaxed);}});ui.add_space(16.);}
    if self.page<=2{card().show(ui,|ui|{ui.set_width(ui.available_width());self.results(ui);});}
    if !self.status.is_empty(){ui.add_space(10.);ui.label(&self.status);}
   });
  });
        if self.confirm {
            egui::Window::new("Confirmar envio ao Lixo").collapsible(false).show(ctx,|ui|{let items=self.chosen();ui.label(format!("Mover {} arquivos ({:.2} MiB) ao Lixo? Revise os caminhos selecionados. Não há exclusão permanente.",items.len(),items.iter().map(|i|i.size).sum::<u64>() as f64/1048576.));if ui.button("Confirmo: mover ao Lixo").clicked(){let results=act(&items,true,trash);self.action_paths=results.iter().map(|(p,_)|p.clone()).collect();self.outcomes=results.iter().enumerate().map(|(n,(_,r))|format!("Item {}: {}",n+1,match r{Ok(_)=>"Movido ao Lixo".into(),Err(e)=>format!("Falhou: {e}")})).collect();self.status="Ações concluídas. Analise novamente antes de outra operação.".into();self.selected.clear();self.report=None;self.confirm=false;}if ui.button("Cancelar operação").clicked(){self.confirm=false;}});
        }
        if self.log_confirm {
            egui::Window::new("Log opcional").collapsible(false).show(ctx,|ui|{ui.label("Será salvo apenas um resumo da análise e resultados das ações. Sem nomes ou caminhos por padrão. O arquivo fica no destino escolhido; nenhuma transmissão é feita.");ui.checkbox(&mut self.paths_in_log,"Incluir explicitamente os caminhos dos resultados (dados sensíveis)");if ui.button("Criar log local…").clicked(){if let Some(p)=rfd::FileDialog::new().set_file_name("sissy-log.txt").save_file(){let mut text=if let Some(r)=&self.report{format!("Examinados: {}; candidatos: {}; cancelada: {}; avisos: {}\n",r.visited,r.items.len(),r.cancelled,r.errors.len())}else{"Sem análise atual\n".into()};text.push_str(&self.outcomes.join("\n"));if self.paths_in_log{for p in &self.action_paths{text.push_str(&format!("\nAção: {}",p.display()));}if let Some(r)=&self.report{for i in &r.items{text.push_str(&format!("\n{}",i.path.display()));}}}self.status=match std::fs::write(p,text){Ok(_)=>"Log salvo localmente".into(),Err(e)=>format!("Falha ao salvar log: {e}")};}self.log_confirm=false;}if ui.button("Cancelar log").clicked(){self.log_confirm=false;}});
        }
    }
}
fn main() -> eframe::Result {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let roots = ["Downloads", "Documents", "Desktop"]
        .iter()
        .map(|d| home.join(d))
        .filter(|p| p.is_dir())
        .collect();
    let image = image::load_from_memory(include_bytes!(
        "../assets/SISSY_ICON_MAC.icon/Assets/Cheerful Lime Snake Mascot.png"
    ))
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
                .with_inner_size([1180., 900.])
                .with_min_inner_size([980., 720.])
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
            cc.egui_ctx.set_fonts(fonts);
            cc.egui_ctx.style_mut(|style| {
                style.spacing.item_spacing = egui::vec2(10., 8.);
                style.spacing.button_padding = egui::vec2(12., 8.);
                style
                    .text_styles
                    .insert(egui::TextStyle::Body, egui::FontId::proportional(16.));
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(16.));
                style
                    .text_styles
                    .insert(egui::TextStyle::Small, egui::FontId::proportional(13.));
            });
            let mut v = egui::Visuals::light();
            v.panel_fill = egui::Color32::WHITE;
            v.selection.bg_fill = egui::Color32::from_rgb(204, 255, 0);
            v.widgets.inactive.bg_fill = egui::Color32::from_rgb(204, 255, 0);
            v.widgets.inactive.bg_stroke = egui::Stroke::new(2_f32, egui::Color32::BLACK);
            v.override_text_color = Some(egui::Color32::BLACK);
            v.window_shadow = egui::epaint::Shadow {
                offset: [5, 5],
                blur: 0,
                spread: 0,
                color: egui::Color32::BLACK,
            };
            cc.egui_ctx.set_visuals(v);
            let mascot = cc.egui_ctx.load_texture(
                "Sissy",
                egui::ColorImage::from_rgba_unmultiplied(dimensions, image.as_raw()),
                Default::default(),
            );
            Ok(Box::new(App {
                mascot: Some(mascot),
                roots,
                large: 500,
                days: 90,
                ..Default::default()
            }))
        }),
    )
}
