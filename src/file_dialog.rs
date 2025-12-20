use std::path::Path;

//
// Copyright 2022-Present (c) Raja Lehtihet & Wael El Oraiby
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
// this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
// this list of conditions and the following disclaimer in the documentation
// and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
// may be used to endorse or promote products derived from this software without
// specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
use crate::*;

/// Simple modal dialog that lets the user browse and pick files.
pub struct FileDialogState {
    current_working_directory: String,
    file_name: Option<String>,
    tmp_file_name: String,
    selected_folder: Option<String>,
    win: WindowHandle,
    folder_panel: ContainerHandle,
    file_panel: ContainerHandle,
    folders: Vec<String>,
    files: Vec<String>,
}

impl FileDialogState {
    /// Returns the selected file name if the dialog completed successfully.
    pub fn file_name(&self) -> &Option<String> { &self.file_name }

    fn list_item_with_icon(container: &mut Container, label: &str, icon: IconId, opt: WidgetOption) -> ResourceState {
        let mut res = ResourceState::NONE;
        let id: Id = if label.is_empty() {
            container.idmngr.get_id_from_str("!list_item_icon")
        } else {
            container.idmngr.get_id_from_str(label)
        };
        let item_rect = container.layout.next();
        let _ = container.update_control(id, item_rect, opt, WidgetBehaviourOption::NONE);
        if container.input.borrow().mouse_pressed.is_left() && container.focus == Some(id) {
            res |= ResourceState::SUBMIT;
        }

        if container.focus == Some(id) || container.hover == Some(id) {
            let mut color = ControlColor::Button;
            if container.focus == Some(id) {
                color.focus();
            } else {
                color.hover();
            }
            let fill = container.style.colors[color as usize];
            container.draw_rect(item_rect, fill);
        }

        let padding = container.style.padding.max(0);
        let icon_size = container.atlas.get_icon_size(icon);
        let icon_x = item_rect.x + padding;
        let icon_y = item_rect.y + ((item_rect.height - icon_size.height) / 2).max(0);
        let icon_rect = rect(icon_x, icon_y, icon_size.width, icon_size.height);
        let mut text_rect = item_rect;
        let consumed = icon_size.width + padding * 2;
        text_rect.x += consumed;
        text_rect.width = (text_rect.width - consumed).max(0);

        let color = container.style.colors[ControlColor::Text as usize];
        container.draw_icon(icon, icon_rect, color);
        if !label.is_empty() {
            container.draw_control_text(label, text_rect, ControlColor::Text, opt);
        }

        res
    }

    fn list_folders_files(p: &Path, folders: &mut Vec<String>, files: &mut Vec<String>) {
        folders.clear();
        files.clear();
        if let Some(parent) = p.parent() {
            folders.push(parent.to_string_lossy().to_string());
        }
        if let Ok(read_dir) = std::fs::read_dir(p) {
            for entry in read_dir {
                if let Ok(e) = entry {
                    let path = e.path();
                    if path.is_dir() {
                        folders.push(path.to_string_lossy().to_string());
                    } else {
                        files.push(e.file_name().to_string_lossy().to_string())
                    }
                }
            }
        }
    }

    /// Creates a new dialog window and associated panels.
    pub fn new<R: Renderer>(ctx: &mut Context<R>) -> Self {
        let mut folders = Vec::new();
        let mut files = Vec::new();
        let current_working_directory = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        Self::list_folders_files(Path::new(&current_working_directory), &mut folders, &mut files);
        Self {
            current_working_directory,
            file_name: None,
            tmp_file_name: String::new(),
            selected_folder: None,
            win: ctx.new_dialog("File Dialog", Recti::new(50, 50, 500, 500)),
            folder_panel: ctx.new_panel("folders"),
            file_panel: ctx.new_panel("files"),
            folders,
            files,
        }
    }

    /// Marks the dialog as open for the next frame.
    pub fn open<R: Renderer>(&mut self, ctx: &mut Context<R>) { ctx.open_dialog(&mut self.win); }

    /// Renders the dialog and updates the selected file when confirmed.
    pub fn eval<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.dialog(&mut self.win, ContainerOption::NONE, WidgetBehaviourOption::NONE, |cont| {
            let mut dialog_state = WindowState::Open;
            let half_width = cont.body.width / 2;
            let parent_path = Path::new(&self.current_working_directory)
                .parent()
                .map(|p| p.to_string_lossy().to_string());
            cont.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |cont| {
                cont.label(&self.current_working_directory);
                cont.textbox_ex(&mut self.tmp_file_name, WidgetOption::NONE);
            });
            let left_column = if half_width > 0 {
                SizePolicy::Remainder(half_width - 1)
            } else {
                SizePolicy::Auto
            };
            let top_row_widths = [left_column, SizePolicy::Remainder(0)];
            cont.with_row(&top_row_widths, SizePolicy::Remainder(24), |cont| {
                cont.panel(&mut self.folder_panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |container_handle| {
                    let container = &mut container_handle.inner_mut();

                    container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                        let mut refresh = false;
                        for f in &self.folders {
                            let label = if parent_path.as_deref() == Some(f.as_str()) {
                                ".."
                            } else {
                                Path::new(f)
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or(f.as_str())
                            };

                            let icon = if self.selected_folder.as_deref() == Some(f) {
                                OPEN_FOLDER_16_ICON
                            } else {
                                CLOSED_FOLDER_16_ICON
                            };

                            if Self::list_item_with_icon(container, label, icon, WidgetOption::NONE).is_submitted() {
                                self.current_working_directory = f.to_string();
                                self.selected_folder = Some(f.to_string());
                                refresh = true;
                            }
                        }
                        if refresh {
                            Self::list_folders_files(&Path::new(&self.current_working_directory), &mut self.folders, &mut self.files);
                        }
                    });
                });
                cont.panel(&mut self.file_panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |container_handle| {
                    let container = &mut container_handle.inner_mut();

                    container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                        if self.files.len() != 0 {
                            for f in &self.files {
                                if Self::list_item_with_icon(container, f, FILE_16_ICON, WidgetOption::NONE).is_submitted() {
                                    self.tmp_file_name = f.to_string();
                                }
                            }
                        } else {
                            container.label("No Files");
                        }
                    });
                });
            });
            let bottom_row_widths = [left_column, SizePolicy::Remainder(0)];
            cont.with_row(&bottom_row_widths, SizePolicy::Remainder(0), |cont| {
                if cont.button_ex("Ok", None, WidgetOption::NONE).is_submitted() {
                    if self.tmp_file_name != "" {
                        self.file_name = Some(self.tmp_file_name.clone())
                    }
                    dialog_state = WindowState::Closed;
                }
                if cont.button_ex("Cancel", None, WidgetOption::NONE).is_submitted() {
                    self.file_name = None;
                    dialog_state = WindowState::Closed;
                }
            });
            dialog_state
        });
    }
}
