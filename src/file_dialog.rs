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

pub struct FileDialogState {
    current_working_directory: String,
    file_name: Option<String>,
    tmp_file_name: String,
    win: WindowHandle,
    folder_panel: ContainerHandle,
    file_panel: ContainerHandle,
    folders: Vec<String>,
    files: Vec<String>,
}

impl FileDialogState {
    pub fn file_name(&self) -> &Option<String> {
        &self.file_name
    }

    fn list_folders_files(p: &Path, folders: &mut Vec<String>, files: &mut Vec<String>) {
        folders.clear();
        files.clear();
        folders.push(p.to_string_lossy().to_string() + "/..");
        for entry in std::fs::read_dir(p).unwrap() {
            let e = entry.unwrap();
            let path = e.path();
            if path.is_dir() {
                folders.push(path.to_str().unwrap().to_string());
            } else {
                files.push(e.file_name().to_str().unwrap().to_string())
            }
        }
    }

    pub fn new<R: Renderer>(ctx: &mut Context<R>) -> Self {
        let mut folders = Vec::new();
        let mut files = Vec::new();
        let current_working_directory = std::env::current_dir().unwrap().clone().to_string_lossy().to_string();
        Self::list_folders_files(Path::new(&current_working_directory), &mut folders, &mut files);
        Self {
            current_working_directory,
            file_name: None,
            tmp_file_name: String::new(),
            win: ctx.new_dialog("File Dialog", Recti::new(50, 50, 500, 500)),
            folder_panel: ctx.new_panel("folders"),
            file_panel: ctx.new_panel("files"),
            folders,
            files,
        }
    }

    pub fn open<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.open_dialog(&mut self.win);
    }

    pub fn eval<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.dialog(&mut self.win, ContainerOption::NONE, |cont| {
            let content_size = cont.content_size;
            let half_width = content_size.x / 2;
            cont.set_row_widths_height(&[-1], 0);
            cont.label(&self.current_working_directory);
            cont.textbox_ex(&mut self.tmp_file_name, WidgetOption::NONE);
            cont.set_row_widths_height(&[-half_width, -1], -25);
            cont.column(|container| {
                container.set_row_widths_height(&[-1], -1);
                container.panel(&mut self.folder_panel, ContainerOption::NONE, |container_handle| {
                    let container = &mut container_handle.inner_mut();

                    container.set_row_widths_height(&[-1], 0);
                    let mut refresh = false;
                    for f in &self.folders {
                        let path = f.split("/").last();

                        if container.button_ex(path.unwrap(), None, WidgetOption::NONE).is_submitted() {
                            self.current_working_directory = f.to_string();
                            refresh = true;
                        }
                    }
                    if refresh {
                        Self::list_folders_files(&Path::new(&self.current_working_directory), &mut self.folders, &mut self.files);
                    }
                });
            });
            cont.column(|container| {
                container.set_row_widths_height(&[-1], -1);
                container.panel(&mut self.file_panel, ContainerOption::NONE, |container_handle| {
                    let container = &mut container_handle.inner_mut();

                    container.set_row_widths_height(&[-1], 0);
                    if self.files.len() != 0 {
                        for f in &self.files {
                            if container.button_ex(f, None, WidgetOption::NONE).is_submitted() {
                                self.tmp_file_name = f.to_string();
                            }
                        }
                    } else {
                        container.label("No Files");
                    }
                });
            });
            cont.set_row_widths_height(&[-half_width, -1], -1);
            if cont.button_ex("Ok", None, WidgetOption::NONE).is_submitted() {
                if self.tmp_file_name != "" {
                    self.file_name = Some(self.tmp_file_name.clone())
                }
                return WindowState::Closed;
            }
            if cont.button_ex("Cancel", None, WidgetOption::NONE).is_submitted() {
                self.file_name = None;
                return WindowState::Closed;
            }
            WindowState::Open
        });
    }
}
