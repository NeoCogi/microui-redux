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
    tmp_file_name: Textbox,
    selected_folder: Option<String>,
    win: WindowHandle,
    folder_panel: ContainerHandle,
    file_panel: ContainerHandle,
    folders: Vec<String>,
    files: Vec<String>,
    folder_items: Vec<ListItem>,
    file_items: Vec<ListItem>,
    ok_button: Button,
    cancel_button: Button,
}

impl FileDialogState {
    /// Returns the selected file name if the dialog completed successfully.
    pub fn file_name(&self) -> &Option<String> { &self.file_name }

    /// Returns `true` if the dialog window is currently open.
    pub fn is_open(&self) -> bool { self.win.is_open() }

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

    fn refresh_entries(&mut self) {
        Self::list_folders_files(Path::new(&self.current_working_directory), &mut self.folders, &mut self.files);
        self.rebuild_item_states();
    }

    fn rebuild_item_states(&mut self) {
        let parent_path = Path::new(&self.current_working_directory).parent().map(|p| p.to_string_lossy().to_string());

        self.folder_items.clear();
        self.folder_items.reserve(self.folders.len());
        for f in &self.folders {
            let label = if parent_path.as_deref() == Some(f.as_str()) {
                ".."
            } else {
                Path::new(f).file_name().and_then(|name| name.to_str()).unwrap_or(f.as_str())
            };
            let icon = if self.selected_folder.as_deref() == Some(f.as_str()) {
                OPEN_FOLDER_16_ICON
            } else {
                CLOSED_FOLDER_16_ICON
            };
            let mut state = ListItem::new(label);
            state.icon = Some(icon);
            self.folder_items.push(state);
        }

        self.file_items.clear();
        self.file_items.reserve(self.files.len());
        for f in &self.files {
            let mut state = ListItem::new(f.as_str());
            state.icon = Some(FILE_16_ICON);
            self.file_items.push(state);
        }
    }

    /// Creates a new dialog window and associated panels.
    pub fn new<R: Renderer>(ctx: &mut Context<R>) -> Self {
        let current_working_directory = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        let mut dialog = Self {
            current_working_directory,
            file_name: None,
            tmp_file_name: Textbox::new(""),
            selected_folder: None,
            win: ctx.new_dialog("File Dialog", Recti::new(50, 50, 500, 500)),
            folder_panel: ctx.new_panel("folders"),
            file_panel: ctx.new_panel("files"),
            folders: Vec::new(),
            files: Vec::new(),
            folder_items: Vec::new(),
            file_items: Vec::new(),
            ok_button: Button::new("Ok"),
            cancel_button: Button::new("Cancel"),
        };
        dialog.refresh_entries();
        dialog
    }

    /// Marks the dialog as open for the next frame.
    pub fn open<R: Renderer>(&mut self, ctx: &mut Context<R>) { ctx.open_dialog(&mut self.win); }

    /// Renders the dialog and updates the selected file when confirmed.
    pub fn eval<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        let mut needs_refresh = false;
        {
            let win = &mut self.win;
            let folder_panel = &mut self.folder_panel;
            let file_panel = &mut self.file_panel;
            let folders = &self.folders;
            let files = &self.files;
            let folder_items = &mut self.folder_items;
            let file_items = &mut self.file_items;
            let current_working_directory = &mut self.current_working_directory;
            let tmp_file_name = &mut self.tmp_file_name;
            let selected_folder = &mut self.selected_folder;
            let ok_button = &mut self.ok_button;
            let cancel_button = &mut self.cancel_button;
            let file_name = &mut self.file_name;

            ctx.dialog(win, ContainerOption::NONE, WidgetBehaviourOption::NONE, |cont| {
                let mut dialog_state = WindowState::Open;
                let half_width = cont.body().width / 2;
                cont.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |cont| {
                    cont.label(current_working_directory.as_str());
                    cont.textbox(tmp_file_name);
                });
                let left_column = if half_width > 0 {
                    SizePolicy::Remainder(half_width - 1)
                } else {
                    SizePolicy::Auto
                };
                let top_row_widths = [left_column, SizePolicy::Remainder(0)];
                cont.with_row(&top_row_widths, SizePolicy::Remainder(24), |cont| {
                    cont.panel(folder_panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |container_handle| {
                        container_handle.with_mut(|container| {
                            container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                                let mut refresh = false;
                                for index in 0..folder_items.len() {
                                    let submitted = {
                                        let item = &mut folder_items[index];
                                        container.list_item(item).is_submitted()
                                    };
                                    if submitted {
                                        if let Some(path) = folders.get(index) {
                                            *current_working_directory = path.to_string();
                                            *selected_folder = Some(path.to_string());
                                        }
                                        refresh = true;
                                    }
                                }
                                if refresh {
                                    needs_refresh = true;
                                }
                            });
                        });
                    });
                    cont.panel(file_panel, ContainerOption::NONE, WidgetBehaviourOption::NONE, |container_handle| {
                        container_handle.with_mut(|container| {
                            container.with_row(&[SizePolicy::Remainder(0)], SizePolicy::Auto, |container| {
                                if !file_items.is_empty() {
                                    for index in 0..file_items.len() {
                                        let submitted = {
                                            let item = &mut file_items[index];
                                            container.list_item(item).is_submitted()
                                        };
                                        if submitted {
                                            if let Some(name) = files.get(index) {
                                                tmp_file_name.buf = name.to_string();
                                            }
                                        }
                                    }
                                } else {
                                    container.label("No Files");
                                }
                            });
                        });
                    });
                });
                let bottom_row_widths = [left_column, SizePolicy::Remainder(0)];
                cont.with_row(&bottom_row_widths, SizePolicy::Remainder(0), |cont| {
                    if cont.button(ok_button).is_submitted() {
                        if tmp_file_name.buf.is_empty() {
                            *file_name = None;
                        } else {
                            *file_name = Some(tmp_file_name.buf.clone());
                        }
                        dialog_state = WindowState::Closed;
                    }
                    if cont.button(cancel_button).is_submitted() {
                        *file_name = None;
                        dialog_state = WindowState::Closed;
                    }
                });
                dialog_state
            });
        }

        if needs_refresh {
            self.refresh_entries();
        }
    }
}
