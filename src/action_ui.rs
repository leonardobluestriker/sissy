use super::*;
use sissy::operations::{self, Action, PreparedBatch, Stage};
impl App {
    pub(crate) fn operation_busy(&self) -> bool {
        self.plan_receiver.is_some() || self.operation_receiver.is_some()
    }
    pub(crate) fn operation_items(&self) -> Vec<SpaceEntry> {
        let mut entries: Vec<_> = self
            .chosen()
            .into_iter()
            .map(|item| SpaceEntry {
                path: item.path,
                size: item.size,
                is_dir: false,
                modified: item.modified,
            })
            .collect();
        if let Some(report) = &self.report {
            entries.extend(
                report
                    .all_files
                    .iter()
                    .chain(&report.all_folders)
                    .filter(|item| self.rank_selected.contains(&item.path))
                    .cloned(),
            );
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.dedup_by(|a, b| a.path == b.path);
        let folders: Vec<_> = entries
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.clone())
            .collect();
        entries.retain(|e| {
            !folders
                .iter()
                .any(|folder| &e.path != folder && e.path.starts_with(folder))
        });
        entries
    }
    pub(crate) fn prepare_selected(&mut self, mut action: Action) {
        if self.operation_busy() || self.receiver.is_some() || self.pending_action.is_some() {
            return;
        }
        let items = self.operation_items();
        if items.is_empty() {
            return;
        }
        if items.iter().any(|e| e.is_dir)
            && self
                .report
                .as_ref()
                .is_some_and(|r| r.cancelled || !r.errors.is_empty())
        {
            self.status =
                text("Para mover pastas inteiras, conclua uma análise sem avisos primeiro.").into();
            return;
        }
        if let Action::Transfer(base) = &action {
            match operations::next_transfer_directory(base) {
                Ok(path) => action = Action::Transfer(path),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        let (sender, receiver) = mpsc::channel();
        self.plan_receiver = Some(receiver);
        self.pending_action = Some(action.clone());
        self.operation_progress = Arc::new(Mutex::new(operations::Progress::default()));
        self.operation_cancel = Arc::new(AtomicBool::new(false));
        self.status.clear();
        let progress = self.operation_progress.clone();
        let cancel = self.operation_cancel.clone();
        std::thread::spawn(move || {
            let result =
                operations::prepare(items, &cancel, |state| *progress.lock().unwrap() = state)
                    .and_then(|batch| {
                        if let Action::Transfer(directory) = &action {
                            operations::validate_destination(&batch, directory)?;
                        }
                        Ok(batch)
                    })
                    .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    pub(crate) fn poll_operations(&mut self) {
        if let Some(receiver) = &self.plan_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.plan_receiver = None;
                    match result {
                        Ok(batch) => self.prepared = Some(batch),
                        Err(error) => {
                            self.status = error;
                            self.pending_action = None;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.plan_receiver = None;
                    self.pending_action = None;
                    self.status =
                        text("A preparação foi interrompida. Nenhuma ação foi iniciada.").into();
                }
                _ => {}
            }
        }
        if let Some(receiver) = &self.operation_receiver {
            match receiver.try_recv() {
                Ok(results) => {
                    self.operation_receiver = None;
                    self.action_paths =
                        results.iter().map(|result| result.source.clone()).collect();
                    self.outcome_summaries = results
                        .iter()
                        .enumerate()
                        .map(|(index, result)| {
                            sissy::tf!(
                                "Item {}: {}",
                                index + 1,
                                text(if result.result.is_ok() {
                                    "Concluído"
                                } else {
                                    "Falhou"
                                })
                            )
                        })
                        .collect();
                    self.outcomes = results
                        .into_iter()
                        .enumerate()
                        .map(|(index, result)| {
                            let message = match result.result {
                                Ok(()) => match result.target {
                                    Some(path) => {
                                        format!("{} {}", text("Transferido para:"), path.display())
                                    }
                                    None => text("Movido ao Lixo").into(),
                                },
                                Err(error) => {
                                    let message = sissy::tf!("Falhou: {e}", e = error);
                                    if let Some(target) = result.target {
                                        format!(
                                            "{message} · {} {}",
                                            text("Destino:"),
                                            target.display()
                                        )
                                    } else {
                                        message
                                    }
                                }
                            };
                            sissy::tf!("Item {}: {}", index + 1, message)
                        })
                        .collect();
                    let state = self.operation_progress.lock().unwrap().clone();
                    self.status = sissy::tf!(
                        "{} de {} itens processados. Analise novamente antes de outra operação.",
                        state.completed,
                        state.total
                    );
                    self.report = None;
                    self.selected.clear();
                    self.rank_selected.clear();
                    self.rank_scope = None;
                    self.media.clear();
                    self.page = 3;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.operation_receiver = None;
                    self.report = None;
                    self.selected.clear();
                    self.rank_selected.clear();
                    self.status = text("Operação interrompida. Confira os arquivos na origem e no destino antes de continuar.").into();
                }
                _ => {}
            }
        }
    }
    pub(crate) fn operation_windows(&mut self, ctx: &egui::Context) {
        let mut cancel_plan = false;
        let mut execute = false;
        if self.plan_receiver.is_some() || self.operation_receiver.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            let state = self.operation_progress.lock().unwrap().clone();
            egui::Window::new(text("Operação em andamento"))
                .collapsible(false)
                .default_width(520.)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(text(match state.stage {
                            Stage::Preparing => "Conferindo o conteúdo da seleção…",
                            Stage::Validating => "Revalidando os itens antes de mover…",
                            Stage::Trashing => "Enviando itens ao Lixo…",
                            Stage::Moving => "Transferindo para o outro disco…",
                            Stage::Verifying => "Conferindo os arquivos no destino…",
                            Stage::Finished => "Finalizando a operação…",
                        }));
                    });
                    ui.label(sissy::tf!(
                        "{} pastas / {} arquivos",
                        state.folders,
                        state.files
                    ));
                    if state.total > 0 {
                        ui.label(sissy::tf!(
                            "{} de {} itens processados",
                            state.completed,
                            state.total
                        ));
                    }
                    if self.plan_receiver.is_some() {
                        if ui.button(text("Cancelar preparação")).clicked() {
                            cancel_plan = true;
                        }
                    } else if self.operation_cancel.load(Ordering::Relaxed) {
                        ui.label(text(
                            "Aguardando o item atual terminar. Os próximos não serão movidos.",
                        ));
                    } else if ui.button(text("Interromper após o item atual")).clicked() {
                        self.operation_cancel.store(true, Ordering::Relaxed);
                    }
                });
        }
        if let (Some(batch), Some(action)) = (&self.prepared, &self.pending_action) {
            egui::Window::new(text("Revisar operação")).collapsible(false).resizable(true).default_width(620.).show(ctx, |ui| {
                ui.strong(text(match action { Action::Trash => "Enviar seleção ao Lixo", Action::Transfer(_) => "Transferir seleção para outro disco" }));
                ui.label(sissy::tf!("{} itens selecionados • {} pastas / {} arquivos • {}", batch.entries.len(), batch.folders, batch.files, mib(batch.bytes)));
                if batch.folders > 0 {
                    ui.label(text("Pastas selecionadas incluem todas as subpastas e arquivos. Seleções de pastas pai e filha são unificadas."));
                }
                if let Action::Transfer(destination) = action {
                    ui.label(format!("{} {}", text("Destino:"), destination.display()));
                    ui.label(text("Após a transferência, os itens deixam a origem. Nomes repetidos recebem numeração; nada é substituído."));
                    ui.small(text("Se a pasta de transferência já existir, será criada a próxima pasta numerada."));
                } else { ui.label(text("O macOS pode pedir permissão para o Sissy controlar o Finder.")); }
                egui::ScrollArea::vertical().id_salt("operation-confirmation").max_height(250.).show(ui, |ui| {
                    for entry in &batch.entries {
                        ui.strong(format!("{} · {}", mib(entry.item.size), text(if entry.item.is_dir { "Pasta" } else { "Arquivo" })));
                        ui.label(entry.item.path.display().to_string());
                        if matches!(action, Action::Transfer(_)) { ui.small(format!("→ {}", entry.target_name.to_string_lossy())); }
                        ui.separator();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(text(match action { Action::Trash => "Confirmo: mover ao Lixo", Action::Transfer(_) => "Confirmo: transferir" })).clicked() { execute = true; }
                    if ui.button(text("Cancelar operação")).clicked() { cancel_plan = true; }
                });
            });
        }
        if cancel_plan {
            self.operation_cancel.store(true, Ordering::Relaxed);
            self.plan_receiver = None;
            self.prepared = None;
            self.pending_action = None;
        }
        if execute {
            if let (Some(batch), Some(action)) = (self.prepared.take(), self.pending_action.take())
            {
                self.start_operation(batch, action);
            }
        }
    }
    fn start_operation(&mut self, batch: PreparedBatch, action: Action) {
        let (sender, receiver) = mpsc::channel();
        self.operation_receiver = Some(receiver);
        let progress = self.operation_progress.clone();
        let cancel = self.operation_cancel.clone();
        std::thread::spawn(move || {
            let results = operations::execute(&batch, &action, true, &cancel, |state| {
                *progress.lock().unwrap() = state
            });
            let _ = sender.send(results);
        });
    }
}
