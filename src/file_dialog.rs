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
    file_path: Option<String>,
    path_box: WidgetHandle<Textbox>,
    tmp_file_name: WidgetHandle<Textbox>,
    selected_folder: Option<String>,
    win: WindowHandle,
    folder_panel: ContainerHandle,
    file_panel: ContainerHandle,
    folders: Vec<String>,
    files: Vec<String>,
    folder_items: Vec<WidgetHandle<ListItem>>,
    file_items: Vec<WidgetHandle<ListItem>>,
    up_button: WidgetHandle<Button>,
    home_button: WidgetHandle<Button>,
    go_button: WidgetHandle<Button>,
    ok_button: WidgetHandle<Button>,
    cancel_button: WidgetHandle<Button>,
    folders_label: WidgetHandle<ListItem>,
    no_folders_label: WidgetHandle<ListItem>,
    files_label: WidgetHandle<ListItem>,
    no_files_label: WidgetHandle<ListItem>,
    file_name_label: WidgetHandle<ListItem>,
    spacer_label: WidgetHandle<ListItem>,
    tree: WidgetTree,
}

impl FileDialogState {
    /// Returns the selected file name (basename only) if the dialog completed successfully.
    pub fn file_name(&self) -> &Option<String> {
        &self.file_name
    }

    /// Returns the selected file path (absolute when possible) if the dialog completed successfully.
    pub fn file_path(&self) -> &Option<String> {
        &self.file_path
    }

    /// Returns `true` if the dialog window is currently open.
    pub fn is_open(&self) -> bool {
        self.win.is_open()
    }

    fn resolve_selected_path(cwd: &str, file_name: &str) -> String {
        let path = Path::new(file_name);
        if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            Path::new(cwd).join(path).to_string_lossy().to_string()
        }
    }

    fn resolve_directory_path(cwd: &str, input: &str) -> Option<String> {
        if input.trim().is_empty() {
            return None;
        }
        let raw = Path::new(input.trim());
        let candidate = if raw.is_absolute() { raw.to_path_buf() } else { Path::new(cwd).join(raw) };
        if candidate.is_dir() {
            Some(candidate.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn home_dir() -> Option<String> {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return Some(home);
            }
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            if !home.is_empty() {
                return Some(home);
            }
        }
        None
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

    fn refresh_entries(&mut self) {
        // Re-snapshot the filesystem, then rebuild both the retained widget
        // handles so list length changes stay in sync.
        Self::list_folders_files(Path::new(&self.current_working_directory), &mut self.folders, &mut self.files);
        self.rebuild_item_states();
    }

    fn rebuild_item_states(&mut self) {
        let parent_path = Path::new(&self.current_working_directory).parent().map(|p| p.to_string_lossy().to_string());

        self.folder_items.clear();
        self.folder_items.reserve(self.folders.len());
        for f in &self.folders {
            // Show the injected parent entry using the conventional ".." label
            // while preserving the full path internally for navigation.
            let label = if parent_path.as_deref() == Some(f.as_str()) {
                ".."
            } else {
                Path::new(f).file_name().and_then(|name| name.to_str()).unwrap_or(f.as_str())
            };
            // Mirror the currently selected directory in the icon so the list
            // provides a visual cue before the next refresh swaps contents.
            let icon = if self.selected_folder.as_deref() == Some(f.as_str()) {
                OPEN_FOLDER_16_ICON
            } else {
                CLOSED_FOLDER_16_ICON
            };
            let mut state = ListItem::new(label);
            state.icon = Some(icon);
            self.folder_items.push(widget_handle(state));
        }

        self.file_items.clear();
        self.file_items.reserve(self.files.len());
        for f in &self.files {
            let mut state = ListItem::new(f.as_str());
            state.icon = Some(FILE_16_ICON);
            self.file_items.push(widget_handle(state));
        }
    }

    fn rebuild_tree(&mut self) {
        let folder_panel = self.folder_panel.clone();
        let file_panel = self.file_panel.clone();
        let up_button = self.up_button.clone();
        let home_button = self.home_button.clone();
        let path_box = self.path_box.clone();
        let go_button = self.go_button.clone();
        let folders_label = self.folders_label.clone();
        let no_folders_label = self.no_folders_label.clone();
        let files_label = self.files_label.clone();
        let no_files_label = self.no_files_label.clone();
        let file_name_label = self.file_name_label.clone();
        let tmp_file_name = self.tmp_file_name.clone();
        let spacer_label = self.spacer_label.clone();
        let cancel_button = self.cancel_button.clone();
        let ok_button = self.ok_button.clone();
        let folder_items = self.folder_items.clone();
        let file_items = self.file_items.clone();
        let no_folder_items = folder_items.is_empty();
        let no_file_items = file_items.is_empty();
        let (control_height, spacing) = {
            let win = self.win.inner();
            let container = &win.main;
            let style = container.style.as_ref();
            let padding = style.padding.max(0);
            let font_height = container.atlas.get_font_height(style.font) as i32;
            let vertical_pad = std::cmp::max(1, padding / 2);
            let icon_height = container.atlas.get_icon_size(EXPAND_DOWN_ICON).height;
            (
                std::cmp::max(font_height + vertical_pad * 2, icon_height),
                style.spacing.max(0),
            )
        };

        self.tree = WidgetTreeBuilder::build(|tree| {
            let toolbar_widths = [SizePolicy::Fixed(56), SizePolicy::Fixed(56), SizePolicy::Remainder(56 + spacing), SizePolicy::Fixed(56)];
            let pane_widths = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];
            let filename_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(0)];
            let action_widths = [SizePolicy::Remainder(96 * 2 + spacing * 2), SizePolicy::Fixed(96), SizePolicy::Fixed(96)];
            let footer_reserved = control_height * 2 + spacing * 2;
            tree.row(&toolbar_widths, SizePolicy::Auto, |tree| {
                tree.widget(up_button.clone());
                tree.widget(home_button.clone());
                tree.widget(path_box.clone());
                tree.widget(go_button.clone());
            });

            tree.row(&pane_widths, SizePolicy::Remainder(footer_reserved), |tree| {
                tree.container(folder_panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                        tree.widget(folders_label.clone());
                        for item in &folder_items {
                            tree.widget(item.clone());
                        }
                        if no_folder_items {
                            tree.widget(no_folders_label.clone());
                        }
                    });
                });

                tree.container(file_panel.clone(), ContainerOption::NONE, WidgetBehaviourOption::NONE, |tree| {
                    tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                        tree.widget(files_label.clone());
                        for item in &file_items {
                            tree.widget(item.clone());
                        }
                        if no_file_items {
                            tree.widget(no_files_label.clone());
                        }
                    });
                });
            });

            tree.row(&filename_widths, SizePolicy::Auto, |tree| {
                tree.widget(file_name_label.clone());
                tree.widget(tmp_file_name.clone());
            });

            tree.row(&action_widths, SizePolicy::Auto, |tree| {
                tree.widget(spacer_label.clone());
                tree.widget(cancel_button.clone());
                tree.widget(ok_button.clone());
            });
        });
    }

    fn sync_retained_view(&mut self) {
        if self.path_box.borrow().buf != self.current_working_directory {
            self.path_box.borrow_mut().buf = self.current_working_directory.clone();
        }
        self.rebuild_tree();
    }

    fn navigate_to(&mut self, path: String) -> bool {
        if path.is_empty() || path == self.current_working_directory {
            return false;
        }
        self.current_working_directory = path;
        self.selected_folder = None;
        self.path_box.borrow_mut().buf = self.current_working_directory.clone();
        true
    }

    fn render<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.dialog(&mut self.win, ContainerOption::NONE, WidgetBehaviourOption::NO_SCROLL, &self.tree);
    }

    fn apply_navigation_actions(&mut self, results: FrameResultGeneration<'_>) -> bool {
        if results.state_of_handle(&self.up_button).is_submitted() {
            if let Some(parent) = Path::new(self.current_working_directory.as_str()).parent() {
                return self.navigate_to(parent.to_string_lossy().to_string());
            }
        }

        if results.state_of_handle(&self.home_button).is_submitted() {
            if let Some(home) = Self::home_dir() {
                if Path::new(home.as_str()).is_dir() {
                    return self.navigate_to(home);
                }
            }
        }

        if results.state_of_handle(&self.path_box).is_submitted() || results.state_of_handle(&self.go_button).is_submitted() {
            let path_input = self.path_box.borrow().buf.clone();
            if let Some(path) = Self::resolve_directory_path(self.current_working_directory.as_str(), path_input.as_str()) {
                return self.navigate_to(path);
            }
        }

        false
    }

    fn apply_folder_actions(&mut self, results: FrameResultGeneration<'_>) -> bool {
        let next_directory = self.folder_items.iter().enumerate().find_map(|(index, item)| {
            if results.state_of_handle(item).is_submitted() {
                self.folders.get(index).cloned()
            } else {
                None
            }
        });

        if let Some(path) = next_directory {
            self.selected_folder = Some(path.clone());
            return self.navigate_to(path);
        }

        false
    }

    fn apply_file_actions(&mut self, results: FrameResultGeneration<'_>) {
        let selected_file = self.file_items.iter().enumerate().find_map(|(index, item)| {
            if results.state_of_handle(item).is_submitted() {
                self.files.get(index).cloned()
            } else {
                None
            }
        });

        if let Some(name) = selected_file {
            self.tmp_file_name.borrow_mut().buf = name;
        }
    }

    fn apply_completion_actions(&mut self, results: FrameResultGeneration<'_>) {
        if results.state_of_handle(&self.cancel_button).is_submitted() {
            self.file_name = None;
            self.file_path = None;
            self.win.close();
        }

        if results.state_of_handle(&self.ok_button).is_submitted() {
            let typed_name = self.tmp_file_name.borrow().buf.clone();
            if typed_name.is_empty() {
                self.file_name = None;
                self.file_path = None;
            } else {
                let selected_path = Self::resolve_selected_path(self.current_working_directory.as_str(), typed_name.as_str());
                let selected_name = Path::new(selected_path.as_str())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
                    .unwrap_or(typed_name);
                self.file_name = Some(selected_name);
                self.file_path = Some(selected_path);
            }
            self.win.close();
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
            file_path: None,
            path_box: widget_handle(Textbox::new("")),
            tmp_file_name: widget_handle(Textbox::new("")),
            selected_folder: None,
            win: ctx.new_dialog("Open File", Recti::new(50, 50, 720, 520)),
            folder_panel: ctx.new_panel("folders"),
            file_panel: ctx.new_panel("files"),
            folders: Vec::new(),
            files: Vec::new(),
            folder_items: Vec::new(),
            file_items: Vec::new(),
            up_button: widget_handle(Button::new("Up")),
            home_button: widget_handle(Button::new("Home")),
            go_button: widget_handle(Button::new("Go")),
            ok_button: widget_handle(Button::new("Open")),
            cancel_button: widget_handle(Button::new("Cancel")),
            folders_label: widget_handle(ListItem::with_opt("Folders", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            no_folders_label: widget_handle(ListItem::with_opt("No folders", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            files_label: widget_handle(ListItem::with_opt("Files", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            no_files_label: widget_handle(ListItem::with_opt("No Files", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            file_name_label: widget_handle(ListItem::with_opt("File name:", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            spacer_label: widget_handle(ListItem::with_opt("", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            tree: WidgetTree::default(),
        };
        dialog.path_box.borrow_mut().buf = dialog.current_working_directory.clone();
        dialog.refresh_entries();
        dialog
    }

    /// Marks the dialog as open for the next frame.
    pub fn open<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.open_dialog(&mut self.win);
    }

    /// Renders the dialog and updates the selected file when confirmed.
    pub fn eval<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        self.sync_retained_view();
        self.render(ctx);
        let results = ctx.committed_results();
        let needs_refresh = self.apply_navigation_actions(results) || self.apply_folder_actions(results);
        self.apply_file_actions(results);
        self.apply_completion_actions(results);

        if needs_refresh {
            // Defer the rebuild until the dialog callback is done so all borrows
            // against the current tree and item handles have been released.
            self.refresh_entries();
        }
    }
}
