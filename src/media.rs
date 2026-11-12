//! Bounded, asynchronous thumbnail loading. Previews stay in the Sissy process.
use eframe::egui;
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::mpsc,
    time::SystemTime,
};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sissy_thumbnail(path: *const u8, len: usize, out_len: *mut usize) -> *mut u8;
    fn sissy_media_free(bytes: *mut u8);
    fn sissy_preview(
        path: *const u8,
        len: usize,
        title: *const u8,
        title_len: usize,
        error: *const u8,
        error_len: usize,
    ) -> i32;
}

fn allowed(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && std::fs::canonicalize(path).is_ok_and(|p| p == path && !sissy::protected(&p))
}

pub fn preview(path: &Path) -> Result<(), String> {
    let error = sissy::i18n::text(
        "Prévia indisponível. O formato pode não ser compatível ou o arquivo não está acessível.",
    );
    if !allowed(path) {
        return Err(error.into());
    }
    #[cfg(target_os = "macos")]
    {
        let path = path.to_str().ok_or_else(|| error.to_owned())?;
        let title = sissy::i18n::text("Prévia no Sissy");
        if unsafe {
            sissy_preview(
                path.as_ptr(),
                path.len(),
                title.as_ptr(),
                title.len(),
                error.as_ptr(),
                error.len(),
            )
        } != 0
        {
            return Ok(());
        }
    }
    Err(error.into())
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
}
struct Reply {
    ticket: u64,
    png: Option<Vec<u8>>,
}
struct Entry {
    ticket: u64,
    texture: Option<egui::TextureHandle>,
    done: bool,
}

pub struct MediaCache {
    sender: mpsc::SyncSender<(u64, PathBuf)>,
    receiver: mpsc::Receiver<Reply>,
    entries: HashMap<Key, Entry>,
    order: VecDeque<Key>,
    next: u64,
    in_flight: usize,
}
impl Default for MediaCache {
    fn default() -> Self {
        let (sender, work) = mpsc::sync_channel::<(u64, PathBuf)>(4);
        let (done, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((ticket, path)) = work.recv() {
                let png = load_thumbnail(&path);
                if done.send(Reply { ticket, png }).is_err() {
                    break;
                }
            }
        });
        Self {
            sender,
            receiver,
            entries: HashMap::new(),
            order: VecDeque::new(),
            next: 0,
            in_flight: 0,
        }
    }
}
fn load_thumbnail(path: &Path) -> Option<Vec<u8>> {
    if !allowed(path) {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        let path = path.to_str()?;
        let mut len = 0;
        let ptr = unsafe { sissy_thumbnail(path.as_ptr(), path.len(), &mut len) };
        if ptr.is_null() {
            return None;
        }
        // Native code returns a bounded 192px PNG and transfers ownership.
        let result = if len <= 4 * 1024 * 1024 {
            Some(unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec())
        } else {
            None
        };
        unsafe {
            sissy_media_free(ptr);
        }
        return result;
    }
    #[cfg(not(target_os = "macos"))]
    None
}
impl MediaCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(reply) = self.receiver.try_recv() {
            self.in_flight = self.in_flight.saturating_sub(1);
            if let Some(entry) = self.entries.values_mut().find(|e| e.ticket == reply.ticket) {
                entry.texture = reply
                    .png
                    .and_then(|png| image::load_from_memory(&png).ok())
                    .map(|image| {
                        let image = image.to_rgba8();
                        ctx.load_texture(
                            format!("thumbnail-{}", reply.ticket),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    });
                entry.done = true;
            }
            ctx.request_repaint();
        }
        if self.in_flight > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    pub fn thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        size: u64,
        modified: SystemTime,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(56., 56.), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return false;
        }
        let key = Key {
            path: path.into(),
            size,
            modified,
        };
        if !self.entries.contains_key(&key) && self.in_flight < 4 {
            self.next += 1;
            if self.sender.try_send((self.next, path.into())).is_ok() {
                self.in_flight += 1;
                self.entries.insert(
                    key.clone(),
                    Entry {
                        ticket: self.next,
                        texture: None,
                        done: false,
                    },
                );
                self.order.push_back(key.clone());
                while self.order.len() > 128 {
                    if let Some(old) = self.order.pop_front() {
                        self.entries.remove(&old);
                    }
                }
            }
        }
        if let Some(texture) = self.entries.get(&key).and_then(|e| e.texture.as_ref()) {
            let image = egui::Image::new((texture.id(), texture.size_vec2()))
                .fit_to_exact_size(rect.size());
            // Preserve aspect ratio in the fixed thumbnail slot.
            let ratio =
                (rect.width() / texture.size_vec2().x).min(rect.height() / texture.size_vec2().y);
            image.paint_at(
                ui,
                egui::Rect::from_center_size(rect.center(), texture.size_vec2() * ratio),
            );
        } else {
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("FILE")
                .chars()
                .take(5)
                .collect::<String>();
            ui.painter()
                .rect_filled(rect, 6., ui.visuals().faint_bg_color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                ext.to_uppercase(),
                egui::FontId::proportional(12.),
                ui.visuals().text_color(),
            );
        }
        response
            .on_hover_text(sissy::i18n::text("Prévia no Sissy"))
            .clicked()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn preview_rejects_missing_files_and_symlink_substitution() {
        let root = tempfile::tempdir_in(std::env::var("HOME").unwrap()).unwrap();
        let file = root.path().join("image.png");
        std::fs::write(&file, b"fixture").unwrap();
        let file = file.canonicalize().unwrap();
        assert!(allowed(&file));
        let link = root.path().join("link.png");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(!allowed(&link));
        assert!(preview(&link).is_err());
        assert!(preview(&root.path().join("missing")).is_err());
        assert!(!allowed(Path::new("/etc/passwd")));
    }
}
