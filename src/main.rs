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
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(rx) = &self.receiver {
            if let Ok(r) = rx.try_recv() {
                self.report = Some(r);
                self.receiver = None;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        egui::CentralPanel::default().show(ctx,|ui|{
   ui.horizontal(|ui| { if let Some(icon)=&self.mascot {ui.image((icon.id(),egui::vec2(80.,80.)));} ui.heading("SISSY • espaço para escolher"); });ui.label("Análise local. Nenhum arquivo é removido durante a análise. Toda sugestão exige revisão humana.");
   ui.checkbox(&mut self.advanced,"Visualização avançada");
   ui.horizontal(|ui|{if ui.add_enabled(self.receiver.is_none(),egui::Button::new("Adicionar pasta…")).clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.roots.push(p);}}
    if self.receiver.is_none() && ui.button("Limpar pastas").clicked(){self.roots.clear();}});
   for p in &self.roots {ui.label(p.display().to_string());}
   ui.label("Somente pastas do usuário. Links simbólicos e locais protegidos são ignorados. Caches: selecione ~/Library/Caches explicitamente.");
   ui.horizontal(|ui|{ui.label("Grandes (MiB):");ui.add(egui::DragValue::new(&mut self.large).range(1..=1000000));ui.label("Downloads: dias desde modificação");ui.add(egui::DragValue::new(&mut self.days).range(1..=36500));});
   if self.receiver.is_some(){ui.label(format!("Analisando {} arquivos…",self.progress.load(Ordering::Relaxed)));if ui.button("Cancelar").clicked(){self.cancel.store(true,Ordering::Relaxed);}}
   else if ui.add_enabled(!self.roots.is_empty(),egui::Button::new("Analisar pastas listadas")).clicked(){self.start();}
   if let Some(r)=&self.report {
    ui.label(format!("{} arquivos examinados • {} candidatos • {} avisos{}",r.visited,r.items.len(),r.errors.len(),if r.cancelled{" • CANCELADA: resultados parciais"}else{""}));
    egui::ScrollArea::vertical().max_height(340.).show(ui,|ui|{for (i,item) in r.items.iter().enumerate(){ui.separator();let mut checked=self.selected.contains(&i);ui.checkbox(&mut checked,format!("{} — {:.2} MiB",item.path.display(),item.size as f64/1048576.));if checked {self.selected.insert(i);}else{self.selected.remove(&i);}
      for reason in &item.reasons {ui.label(reason);}
      if let Some(k)=&item.keeper {ui.label(format!("Preservar: {}",k.display()));}
      ui.label(format!("Modificado: {} segundos após Unix epoch",item.modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
      ui.horizontal(|ui|{for (label,reveal) in [("Abrir",false),("Mostrar no Finder",true)]{if ui.button(label).clicked(){
       #[cfg(target_os="macos")]
       {let mut cmd=std::process::Command::new("/usr/bin/open");if reveal{cmd.arg("-R");}match cmd.arg(&item.path).status(){Ok(s) if s.success()=>{},_=>self.status="Não foi possível abrir; verifique permissões.".into()}}
       #[cfg(not(target_os="macos"))] {let _=reveal; self.status="Abertura disponível no macOS.".into();}
      }}});
     }if self.advanced {for e in &r.errors{ui.label(e);}}});
   }
   let chosen=self.chosen();let bytes:u64=chosen.iter().map(|i|i.size).sum();
   ui.label(format!("{} selecionados • tamanho lógico {:.2} MiB (espaço físico recuperado pode variar; Lixo precisa ser esvaziado pelo usuário)",chosen.len(),bytes as f64/1048576.));
   if ui.add_enabled(!chosen.is_empty() && self.receiver.is_none(),egui::Button::new("Revisar envio ao Lixo…")).clicked(){self.confirm=true;}
   if ui.button("Registrar análise / ações em log local…").clicked(){self.log_confirm=true;}
   for (n,s) in self.outcomes.iter().enumerate(){ui.label(s);if let Some(p)=self.action_paths.get(n){ui.label(p.display().to_string());}}ui.label(&self.status);
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
                .with_inner_size([1050., 850.])
                .with_icon(icon),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            for p in [
                "/System/Library/Fonts/Supplemental/Comic Sans MS.ttf",
                "/Library/Fonts/Comic Sans MS.ttf",
            ] {
                if let Ok(data) = std::fs::read(p) {
                    fonts
                        .font_data
                        .insert("comic".into(), egui::FontData::from_owned(data).into());
                    fonts
                        .families
                        .get_mut(&egui::FontFamily::Proportional)
                        .unwrap()
                        .insert(0, "comic".into());
                    break;
                }
            }
            cc.egui_ctx.set_fonts(fonts);
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
