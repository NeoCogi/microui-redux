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
//! Retained file picker dialog state and UI node set construction.
//!
//! The dialog owns reusable widget handles for folder/file lists, navigation buttons, path entry,
//! and selection state so applications can open it repeatedly without rebuilding runtime state.
use std::path::Path;

use crate::*;

/// Simple modal dialog that lets the user browse and pick files.
pub struct FileDialogState {
    /// Directory currently shown by the dialog.
    current_working_directory: String,
    /// Selected basename after the user accepts the dialog.
    file_name: Option<String>,
    /// Selected resolved path after the user accepts the dialog.
    file_path: Option<String>,
    /// Editable path textbox.
    path_box: WidgetHandle<Textbox>,
    /// Editable filename textbox.
    tmp_file_name: WidgetHandle<Textbox>,
    /// Folder selected from the folder list, if any.
    selected_folder: Option<String>,
    /// Registered root id for the dialog window.
    root: RootId,
    /// Cached open state mirrored from the registered root.
    open: bool,
    /// Scroll area containing folder rows.
    folder_area: ScrollAreaHandle,
    /// Scroll area containing file rows.
    file_area: ScrollAreaHandle,
    /// Folder names currently displayed.
    folders: Vec<String>,
    /// File names currently displayed.
    files: Vec<String>,
    /// Retained list-item handles for folder rows.
    folder_items: Vec<WidgetHandle<ListItem>>,
    /// Retained list-item handles for file rows.
    file_items: Vec<WidgetHandle<ListItem>>,
    /// Node ids corresponding to folder rows.
    folder_item_ids: Vec<NodeId>,
    /// Node ids corresponding to file rows.
    file_item_ids: Vec<NodeId>,
    /// Button that navigates to the parent directory.
    up_button: WidgetHandle<Button>,
    /// Button that navigates to the home directory.
    home_button: WidgetHandle<Button>,
    /// Button that applies the path textbox.
    go_button: WidgetHandle<Button>,
    /// Button that accepts the current selection.
    ok_button: WidgetHandle<Button>,
    /// Button that closes without selecting.
    cancel_button: WidgetHandle<Button>,
    /// Node id for the parent-directory button.
    up_button_id: NodeId,
    /// Node id for the home-directory button.
    home_button_id: NodeId,
    /// Node id for the path textbox.
    path_box_id: NodeId,
    /// Node id for the path-apply button.
    go_button_id: NodeId,
    /// Node id for the accept button.
    ok_button_id: NodeId,
    /// Node id for the cancel button.
    cancel_button_id: NodeId,
    /// Static label above the folder list.
    folders_label: WidgetHandle<ListItem>,
    /// Placeholder shown when no folders exist.
    no_folders_label: WidgetHandle<ListItem>,
    /// Static label above the file list.
    files_label: WidgetHandle<ListItem>,
    /// Placeholder shown when no files exist.
    no_files_label: WidgetHandle<ListItem>,
    /// Static label for the filename textbox.
    file_name_label: WidgetHandle<ListItem>,
    /// Spacer row used by the layout tree.
    spacer_label: WidgetHandle<ListItem>,
    /// Retained UI node set submitted for the dialog.
    tree: UiNodeSet,
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
        self.open
    }

    /// Resolves a typed file name into a path relative to the current directory when needed.
    fn resolve_selected_path(cwd: &str, file_name: &str) -> String {
        let path = Path::new(file_name);
        if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            Path::new(cwd).join(path).to_string_lossy().to_string()
        }
    }

    /// Resolves a typed directory path and accepts it only when it exists.
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

    /// Returns the best available user home directory from common environment variables.
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

    /// Reads one directory into separate folder and file lists.
    fn list_folders_files(p: &Path, folders: &mut Vec<String>, files: &mut Vec<String>) {
        folders.clear();
        files.clear();
        if let Some(parent) = p.parent() {
            // Inject parent as the first folder entry so the dialog can always navigate upward.
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

    /// Refreshes filesystem entries and rebuilt list item widget handles.
    fn refresh_entries(&mut self) {
        // Re-snapshot the filesystem, then rebuild both the retained widget
        // handles so list length changes stay in sync.
        Self::list_folders_files(Path::new(&self.current_working_directory), &mut self.folders, &mut self.files);
        self.rebuild_item_states();
    }

    /// Rebuilds retained list item state from the latest folder/file names.
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

    /// Rebuilds the retained UI node set and records the node ids used for result lookup.
    fn rebuild_tree(&mut self, control_height: i32, spacing: i32) {
        let mut folder_item_ids = Vec::with_capacity(self.folder_items.len());
        let mut file_item_ids = Vec::with_capacity(self.file_items.len());
        let mut up_button_id = NodeId::default();
        let mut home_button_id = NodeId::default();
        let mut path_box_id = NodeId::default();
        let mut go_button_id = NodeId::default();
        let mut cancel_button_id = NodeId::default();
        let mut ok_button_id = NodeId::default();
        let tree = {
            let folder_area = &self.folder_area;
            let file_area = &self.file_area;
            let up_button = &self.up_button;
            let home_button = &self.home_button;
            let path_box = &self.path_box;
            let go_button = &self.go_button;
            let folders_label = &self.folders_label;
            let no_folders_label = &self.no_folders_label;
            let files_label = &self.files_label;
            let no_files_label = &self.no_files_label;
            let file_name_label = &self.file_name_label;
            let tmp_file_name = &self.tmp_file_name;
            let spacer_label = &self.spacer_label;
            let cancel_button = &self.cancel_button;
            let ok_button = &self.ok_button;
            let folder_items = &self.folder_items;
            let file_items = &self.file_items;
            let no_folder_items = folder_items.is_empty();
            let no_file_items = file_items.is_empty();

            UiNodeBuilder::build(|tree| {
                let toolbar_widths = [
                    SizePolicy::Fixed(56),
                    SizePolicy::Fixed(56),
                    SizePolicy::Remainder(56 + spacing),
                    SizePolicy::Fixed(56),
                ];
                let pane_widths = [SizePolicy::Weight(1.0), SizePolicy::Weight(2.0)];
                let filename_widths = [SizePolicy::Fixed(86), SizePolicy::Remainder(0)];
                let action_widths = [SizePolicy::Remainder(96 * 2 + spacing * 2), SizePolicy::Fixed(96), SizePolicy::Fixed(96)];
                let footer_reserved = control_height * 2 + spacing * 2;
                // Toolbar: up/home/path/go.
                tree.row(&toolbar_widths, SizePolicy::Auto, |tree| {
                    up_button_id = tree.widget(up_button);
                    home_button_id = tree.widget(home_button);
                    path_box_id = tree.widget(path_box);
                    go_button_id = tree.widget(go_button);
                });

                // Main pane: folders on the left, files on the right, both scrollable through scroll areas.
                tree.row(&pane_widths, SizePolicy::Remainder(footer_reserved), |tree| {
                    tree.scroll_area(folder_area, ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                            tree.widget(folders_label);
                            for item in folder_items {
                                folder_item_ids.push(tree.widget(item));
                            }
                            if no_folder_items {
                                tree.widget(no_folders_label);
                            }
                        });
                    });

                    tree.scroll_area(file_area, ContainerOption::NONE, ScrollBehavior::NONE, |tree| {
                        tree.stack(SizePolicy::Remainder(0), SizePolicy::Auto, StackDirection::TopToBottom, |tree| {
                            tree.widget(files_label);
                            for item in file_items {
                                file_item_ids.push(tree.widget(item));
                            }
                            if no_file_items {
                                tree.widget(no_files_label);
                            }
                        });
                    });
                });

                // Filename row and action buttons.
                tree.row(&filename_widths, SizePolicy::Auto, |tree| {
                    tree.widget(file_name_label);
                    tree.widget(tmp_file_name);
                });

                tree.row(&action_widths, SizePolicy::Auto, |tree| {
                    tree.widget(spacer_label);
                    cancel_button_id = tree.widget(cancel_button);
                    ok_button_id = tree.widget(ok_button);
                });
            })
        };
        self.tree = tree;
        self.folder_item_ids = folder_item_ids;
        self.file_item_ids = file_item_ids;
        self.up_button_id = up_button_id;
        self.home_button_id = home_button_id;
        self.path_box_id = path_box_id;
        self.go_button_id = go_button_id;
        self.cancel_button_id = cancel_button_id;
        self.ok_button_id = ok_button_id;
    }

    /// Synchronizes text boxes and tree structure with the current dialog state.
    fn sync_retained_view(&mut self, control_height: i32, spacing: i32) {
        if self.path_box.read(|path_box| path_box.text() != self.current_working_directory) {
            self.path_box.update(|path_box| {
                path_box.set_text(self.current_working_directory.clone());
            });
        }
        self.rebuild_tree(control_height, spacing);
    }

    /// Changes the working directory and resets folder selection.
    fn navigate_to(&mut self, path: String) -> bool {
        if path.is_empty() || path == self.current_working_directory {
            return false;
        }
        self.current_working_directory = path;
        self.selected_folder = None;
        self.path_box.update(|path_box| {
            path_box.set_text(self.current_working_directory.clone());
        });
        self.tmp_file_name.update(|tmp_file_name| {
            tmp_file_name.set_text("");
        });
        true
    }

    /// Pushes the current retained nodes/options into the registered context root.
    fn sync_retained_root<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        let (control_height, spacing) = ctx.root_control_metrics();
        self.sync_retained_view(control_height, spacing);
        ctx.set_root_nodes(self.root, std::mem::take(&mut self.tree));
        self.open = ctx.root_visible(self.root).unwrap_or(false);
    }

    /// Checks whether a node inside the root dialog submitted in the committed results.
    fn root_submitted(&self, results: FrameResultGeneration<'_>, node_id: NodeId) -> bool {
        results.state_of_retained(RetainedId::root_node(self.root, node_id)).is_submitted()
    }

    /// Applies toolbar/path navigation actions from committed frame results.
    fn apply_navigation_actions(&mut self, results: FrameResultGeneration<'_>) -> bool {
        if self.root_submitted(results, self.up_button_id) {
            if let Some(parent) = Path::new(self.current_working_directory.as_str()).parent() {
                return self.navigate_to(parent.to_string_lossy().to_string());
            }
        }

        if self.root_submitted(results, self.home_button_id) {
            if let Some(home) = Self::home_dir() {
                if Path::new(home.as_str()).is_dir() {
                    return self.navigate_to(home);
                }
            }
        }

        if self.root_submitted(results, self.path_box_id) || self.root_submitted(results, self.go_button_id) {
            let path_input = self.path_box.read(|path_box| path_box.text().to_owned());
            if let Some(path) = Self::resolve_directory_path(self.current_working_directory.as_str(), path_input.as_str()) {
                return self.navigate_to(path);
            }
        }

        false
    }

    /// Applies folder-list selection and navigates when a folder is submitted.
    fn apply_folder_actions(&mut self, results: FrameResultGeneration<'_>) -> bool {
        let next_directory = self.folder_item_ids.iter().enumerate().find_map(|(index, node_id)| {
            if self.root_submitted(results, *node_id) {
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

    /// Applies file-list selection into the temporary filename textbox.
    fn apply_file_actions(&mut self, results: FrameResultGeneration<'_>) {
        let selected_file = self.file_item_ids.iter().enumerate().find_map(|(index, node_id)| {
            if self.root_submitted(results, *node_id) {
                self.files.get(index).cloned()
            } else {
                None
            }
        });

        if let Some(name) = selected_file {
            self.tmp_file_name.update(|tmp_file_name| {
                tmp_file_name.set_text(name);
            });
        }
    }

    /// Applies OK/Cancel actions and stores the selected file result.
    fn apply_completion_actions(&mut self, results: FrameResultGeneration<'_>) -> bool {
        let mut close = false;
        if self.root_submitted(results, self.cancel_button_id) {
            self.file_name = None;
            self.file_path = None;
            self.open = false;
            close = true;
        }

        if self.root_submitted(results, self.ok_button_id) {
            let typed_name = self.tmp_file_name.read(|tmp_file_name| tmp_file_name.text().to_owned());
            if typed_name.is_empty() {
                self.file_name = None;
                self.file_path = None;
            } else {
                // Store both the display basename and resolved path so callers can choose either.
                let selected_path = Self::resolve_selected_path(self.current_working_directory.as_str(), typed_name.as_str());
                let selected_name = Path::new(selected_path.as_str())
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
                    .unwrap_or(typed_name);
                self.file_name = Some(selected_name);
                self.file_path = Some(selected_path);
            }
            self.open = false;
            close = true;
        }
        close
    }

    /// Creates a new dialog window and associated scroll areas.
    pub fn new<R: Renderer>(ctx: &mut Context<R>) -> Self {
        let current_working_directory = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        let root = ctx.create_dialog("Open File", Recti::new(50, 50, 720, 520), UiNodeSet::default());
        ctx.set_root_options(root, ContainerOption::NONE, ScrollBehavior::NO_SCROLL);
        let mut dialog = Self {
            current_working_directory,
            file_name: None,
            file_path: None,
            path_box: widget_handle(Textbox::new("")),
            tmp_file_name: widget_handle(Textbox::new("")),
            selected_folder: None,
            root,
            open: ctx.root_visible(root).unwrap_or(false),
            folder_area: ctx.new_scroll_area("folders"),
            file_area: ctx.new_scroll_area("files"),
            folders: Vec::new(),
            files: Vec::new(),
            folder_items: Vec::new(),
            file_items: Vec::new(),
            folder_item_ids: Vec::new(),
            file_item_ids: Vec::new(),
            up_button: widget_handle(Button::new("Up")),
            home_button: widget_handle(Button::new("Home")),
            go_button: widget_handle(Button::new("Go")),
            ok_button: widget_handle(Button::new("Open")),
            cancel_button: widget_handle(Button::new("Cancel")),
            up_button_id: NodeId::default(),
            home_button_id: NodeId::default(),
            path_box_id: NodeId::default(),
            go_button_id: NodeId::default(),
            ok_button_id: NodeId::default(),
            cancel_button_id: NodeId::default(),
            folders_label: widget_handle(ListItem::with_opt("Folders", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            no_folders_label: widget_handle(ListItem::with_opt("No folders", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            files_label: widget_handle(ListItem::with_opt("Files", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            no_files_label: widget_handle(ListItem::with_opt("No Files", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            file_name_label: widget_handle(ListItem::with_opt("File name:", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            spacer_label: widget_handle(ListItem::with_opt("", WidgetOption::NO_INTERACT | WidgetOption::NO_FRAME)),
            tree: UiNodeSet::default(),
        };
        dialog.path_box.update(|path_box| {
            path_box.set_text(dialog.current_working_directory.clone());
        });
        dialog.refresh_entries();
        dialog.sync_retained_root(ctx);
        dialog
    }

    /// Marks the dialog as open for the next frame.
    pub fn open<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        ctx.set_root_visible(self.root, true);
        self.open = true;
    }

    /// Renders the dialog and updates the selected file when confirmed.
    pub fn eval<R: Renderer>(&mut self, ctx: &mut Context<R>) {
        let results = ctx.committed_results();
        let needs_refresh = self.apply_navigation_actions(results) || self.apply_folder_actions(results);
        self.apply_file_actions(results);
        let close = self.apply_completion_actions(results);
        if close {
            ctx.set_root_visible(self.root, false);
        }

        if needs_refresh {
            // Defer the rebuild until the dialog callback is done so all borrows
            // against the current tree and item handles have been released.
            self.refresh_entries();
        }
        self.sync_retained_root(ctx);
        self.open = ctx.root_visible(self.root).unwrap_or(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_atlas, NoopRenderer};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("microui-redux-{name}-{}-{nanos}", std::process::id()))
    }

    fn click_node(ctx: &mut Context<NoopRenderer>, root: RootId, node: NodeId) {
        let rect = ctx.debug_root_node_rect(root, node).expect("node rect should be laid out");
        let x = rect.x + rect.width / 2;
        let y = rect.y + rect.height / 2;
        ctx.mousemove(x, y);
        ctx.update_ui();
        ctx.mousedown(x, y, MouseButton::LEFT);
        ctx.update_ui();
    }

    fn click_node_without_hover_frame(ctx: &mut Context<NoopRenderer>, root: RootId, node: NodeId) {
        let rect = ctx.debug_root_node_rect(root, node).expect("node rect should be laid out");
        let x = rect.x + rect.width / 2;
        let y = rect.y + rect.height / 2;
        ctx.mousemove(x, y);
        ctx.mousedown(x, y, MouseButton::LEFT);
        ctx.update_ui();
    }

    #[test]
    fn selecting_file_row_then_open_returns_selected_path() {
        let dir = unique_temp_dir("file-dialog-select");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("picked.txt");
        fs::write(&file_path, b"picked").unwrap();

        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.current_working_directory = dir.to_string_lossy().to_string();
        dialog.refresh_entries();
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let file_node = dialog.file_item_ids[0];
        click_node(&mut ctx, dialog.root, file_node);
        dialog.eval(&mut ctx);

        assert_eq!(dialog.tmp_file_name.read(|tmp| tmp.text().to_string()), "picked.txt");

        ctx.mouseup(0, 0, MouseButton::LEFT);
        ctx.update_ui();
        let ok_node = dialog.ok_button_id;
        click_node(&mut ctx, dialog.root, ok_node);
        dialog.eval(&mut ctx);

        assert_eq!(dialog.file_name().as_deref(), Some("picked.txt"));
        assert_eq!(dialog.file_path().as_deref(), Some(file_path.to_string_lossy().as_ref()));

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn file_dialog_clicks_work_without_prior_hover_frame() {
        let dir = unique_temp_dir("file-dialog-batched-click");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("batched.txt");
        fs::write(&file_path, b"picked").unwrap();

        let atlas = test_atlas();
        let renderer = RendererHandle::new(NoopRenderer { atlas });
        let mut ctx = Context::new(renderer, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.current_working_directory = dir.to_string_lossy().to_string();
        dialog.refresh_entries();
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let file_node = dialog.file_item_ids[0];
        click_node_without_hover_frame(&mut ctx, dialog.root, file_node);
        dialog.eval(&mut ctx);

        assert_eq!(dialog.tmp_file_name.read(|tmp| tmp.text().to_string()), "batched.txt");

        ctx.mouseup(0, 0, MouseButton::LEFT);
        ctx.update_ui();
        let ok_node = dialog.ok_button_id;
        click_node_without_hover_frame(&mut ctx, dialog.root, ok_node);
        dialog.eval(&mut ctx);

        assert_eq!(dialog.file_name().as_deref(), Some("batched.txt"));
        assert_eq!(dialog.file_path().as_deref(), Some(file_path.to_string_lossy().as_ref()));

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir(dir);
    }
}
