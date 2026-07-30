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

use crate::{render::RendererBackend, *};

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
    /// Typed application state for the path textbox.
    path_box_state: WidgetStateHandle<TextboxState>,
    /// Editable filename textbox.
    tmp_file_name: WidgetHandle<Textbox>,
    /// Typed application state for the filename textbox.
    tmp_file_name_state: WidgetStateHandle<TextboxState>,
    /// Folder selected from the folder list, if any.
    selected_folder: Option<String>,
    /// Registered root id for the dialog window.
    root: RootId,
    /// Cached open state mirrored from the registered root.
    open: bool,
    /// Folder names currently displayed.
    folders: Vec<String>,
    /// File names currently displayed.
    files: Vec<String>,
    /// Retained list-item handles for folder rows.
    folder_items: Vec<WidgetHandle<ListItem>>,
    /// Typed application state for folder rows.
    folder_item_states: Vec<WidgetStateHandle<ListItemState>>,
    /// Retained list-item handles for file rows.
    file_items: Vec<WidgetHandle<ListItem>>,
    /// Typed application state for file rows.
    file_item_states: Vec<WidgetStateHandle<ListItemState>>,
    /// Node ids corresponding to folder rows.
    folder_item_ids: Vec<NodeId>,
    /// Node ids corresponding to file rows.
    file_item_ids: Vec<NodeId>,
    /// Button that navigates to the parent directory.
    up_button: WidgetHandle<Button>,
    /// Typed application state for the parent-directory button.
    up_button_state: WidgetStateHandle<ButtonState>,
    /// Button that navigates to the home directory.
    home_button: WidgetHandle<Button>,
    /// Typed application state for the home-directory button.
    home_button_state: WidgetStateHandle<ButtonState>,
    /// Button that applies the path textbox.
    go_button: WidgetHandle<Button>,
    /// Typed application state for the path-apply button.
    go_button_state: WidgetStateHandle<ButtonState>,
    /// Button that accepts the current selection.
    ok_button: WidgetHandle<Button>,
    /// Typed application state for the accept button.
    ok_button_state: WidgetStateHandle<ButtonState>,
    /// Button that closes without selecting.
    cancel_button: WidgetHandle<Button>,
    /// Typed application state for the cancel button.
    cancel_button_state: WidgetStateHandle<ButtonState>,
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
    /// Builds the temporary projection handle used until owning nodes land in P1.3.
    fn projected_textbox(parameters: TextboxParameters) -> (WidgetStateHandle<TextboxState>, WidgetHandle<Textbox>) {
        let (state, runtime) = Textbox::create(parameters);
        (state, widget_handle(runtime))
    }

    /// Builds a button state/runtime pair across the temporary projection boundary.
    fn projected_button(parameters: ButtonParameters) -> (WidgetStateHandle<ButtonState>, WidgetHandle<Button>) {
        let (state, runtime) = Button::create(parameters);
        (state, widget_handle(runtime))
    }

    /// Builds a list-item state/runtime pair across the temporary projection boundary.
    fn projected_list_item(parameters: ListItemParameters) -> (WidgetStateHandle<ListItemState>, WidgetHandle<ListItem>) {
        let (state, runtime) = ListItem::create(parameters);
        (state, widget_handle(runtime))
    }

    /// Builds a projection-only list item whose mounted state is intentionally unused.
    fn projected_static_list_item(parameters: ListItemParameters) -> WidgetHandle<ListItem> {
        let (_, runtime) = ListItem::create(parameters);
        widget_handle(runtime)
    }

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
        self.folder_item_states.clear();
        self.folder_items.reserve(self.folders.len());
        self.folder_item_states.reserve(self.folders.len());
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
            let (state, runtime) = Self::projected_list_item(ListItemParameters::with_icon(label, icon));
            self.folder_item_states.push(state);
            self.folder_items.push(runtime);
        }

        self.file_items.clear();
        self.file_item_states.clear();
        self.file_items.reserve(self.files.len());
        self.file_item_states.reserve(self.files.len());
        for f in &self.files {
            let (state, runtime) = Self::projected_list_item(ListItemParameters::with_icon(f.as_str(), FILE_16_ICON));
            self.file_item_states.push(state);
            self.file_items.push(runtime);
        }
    }

    /// Rebuilds the retained UI node set and records the node ids used for result lookup.
    fn rebuild_tree(&mut self, spacing: i32) {
        let mut folder_item_ids = Vec::with_capacity(self.folder_items.len());
        let mut file_item_ids = Vec::with_capacity(self.file_items.len());
        let mut up_button_id = NodeId::default();
        let mut home_button_id = NodeId::default();
        let mut path_box_id = NodeId::default();
        let mut go_button_id = NodeId::default();
        let mut cancel_button_id = NodeId::default();
        let mut ok_button_id = NodeId::default();
        let tree = {
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
                // The column owns the complete dialog body. Natural-height controls reserve only
                // what they measure, and the weighted browser row receives every remaining pixel.
                tree.node(NodeOptions::with_policy(Policy::fill())).column(|tree| {
                    // Toolbar: up/home/path/go.
                    tree.row(&toolbar_widths, SizePolicy::Auto, |tree| {
                        up_button_id = tree.widget(up_button);
                        home_button_id = tree.widget(home_button);
                        path_box_id = tree.widget(path_box);
                        go_button_id = tree.widget(go_button);
                    });

                    // Main pane: folders on the left, files on the right, both scrollable through scroll areas.
                    tree.row(&pane_widths, SizePolicy::Weight(1.0), |tree| {
                        tree.scroll_area(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, |tree| {
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

                        tree.scroll_area(ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL, |tree| {
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

                    // Filename row and action buttons remain at their natural control height.
                    tree.row(&filename_widths, SizePolicy::Auto, |tree| {
                        tree.widget(file_name_label);
                        tree.widget(tmp_file_name);
                    });

                    tree.row(&action_widths, SizePolicy::Auto, |tree| {
                        tree.widget(spacer_label);
                        cancel_button_id = tree.widget(cancel_button);
                        ok_button_id = tree.widget(ok_button);
                    });
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
    fn sync_retained_view(&mut self, spacing: i32) {
        if self.path_box_state.try_read(|path_box| path_box.text() != self.current_working_directory) == Some(true) {
            let _ = self
                .path_box_state
                .try_update_with(self.current_working_directory.clone(), |path_box, path| path_box.set_text(path));
        }
        self.rebuild_tree(spacing);
    }

    /// Changes the working directory and resets folder selection.
    fn navigate_to(&mut self, path: String) -> bool {
        if path.is_empty() || path == self.current_working_directory {
            return false;
        }
        self.current_working_directory = path;
        self.selected_folder = None;
        let _ = self
            .path_box_state
            .try_update_with(self.current_working_directory.clone(), |path_box, path| path_box.set_text(path));
        let _ = self.tmp_file_name_state.try_update(|tmp_file_name| tmp_file_name.set_text(""));
        true
    }

    /// Pushes the current retained nodes/options into the registered context root.
    fn sync_retained_root<B: RendererBackend>(&mut self, ctx: &mut Context<B>) {
        self.sync_retained_view(ctx.root_spacing());
        ctx.set_root_nodes(self.root, std::mem::take(&mut self.tree));
        self.open = ctx.root_visible(self.root).unwrap_or(false);
    }

    /// Applies toolbar/path navigation actions from committed frame results.
    fn apply_navigation_actions(&mut self) -> bool {
        if self.up_button_state.try_update(ButtonState::take_submitted).unwrap_or(false)
            && let Some(parent) = Path::new(self.current_working_directory.as_str()).parent()
        {
            return self.navigate_to(parent.to_string_lossy().to_string());
        }

        if self.home_button_state.try_update(ButtonState::take_submitted).unwrap_or(false)
            && let Some(home) = Self::home_dir()
            && Path::new(home.as_str()).is_dir()
        {
            return self.navigate_to(home);
        }

        let path_submitted = self.path_box_state.try_update(TextboxState::take_submitted).unwrap_or(false);
        let go_submitted = self.go_button_state.try_update(ButtonState::take_submitted).unwrap_or(false);
        if path_submitted || go_submitted {
            let path_input = self.path_box_state.try_read(|path_box| path_box.text().to_owned()).unwrap_or_default();
            if let Some(path) = Self::resolve_directory_path(self.current_working_directory.as_str(), path_input.as_str()) {
                return self.navigate_to(path);
            }
        }

        false
    }

    /// Applies folder-list selection and navigates when a folder is submitted.
    fn apply_folder_actions(&mut self) -> bool {
        let next_directory = self.folder_item_states.iter().enumerate().find_map(|(index, state)| {
            if state.try_update(ListItemState::take_submitted).unwrap_or(false) {
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
    fn apply_file_actions(&mut self) {
        let selected_file = self.file_item_states.iter().enumerate().find_map(|(index, state)| {
            if state.try_update(ListItemState::take_submitted).unwrap_or(false) {
                self.files.get(index).cloned()
            } else {
                None
            }
        });

        if let Some(name) = selected_file {
            let _ = self
                .tmp_file_name_state
                .try_update_with(name, |tmp_file_name, name| tmp_file_name.set_text(name));
        }
    }

    /// Applies OK/Cancel actions and stores the selected file result.
    fn apply_completion_actions(&mut self) -> bool {
        let mut close = false;
        if self.cancel_button_state.try_update(ButtonState::take_submitted).unwrap_or(false) {
            self.file_name = None;
            self.file_path = None;
            self.open = false;
            close = true;
        }

        if self.ok_button_state.try_update(ButtonState::take_submitted).unwrap_or(false) {
            let typed_name = self
                .tmp_file_name_state
                .try_read(|tmp_file_name| tmp_file_name.text().to_owned())
                .unwrap_or_default();
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
    pub fn new<B: RendererBackend>(ctx: &mut Context<B>) -> Self {
        let current_working_directory = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();
        let root = ctx.create_dialog("Open File", Recti::new(50, 50, 720, 520), UiNodeSet::default());
        ctx.set_root_options(root, WindowOption::FRAME);
        let (path_box_state, path_box) = Self::projected_textbox(TextboxParameters::new(""));
        let (tmp_file_name_state, tmp_file_name) = Self::projected_textbox(TextboxParameters::new(""));
        let (up_button_state, up_button) = Self::projected_button(ButtonParameters::new("Up"));
        let (home_button_state, home_button) = Self::projected_button(ButtonParameters::new("Home"));
        let (go_button_state, go_button) = Self::projected_button(ButtonParameters::new("Go"));
        let (ok_button_state, ok_button) = Self::projected_button(ButtonParameters::new("Open"));
        let (cancel_button_state, cancel_button) = Self::projected_button(ButtonParameters::new("Cancel"));
        let mut dialog = Self {
            current_working_directory,
            file_name: None,
            file_path: None,
            path_box,
            path_box_state,
            tmp_file_name,
            tmp_file_name_state,
            selected_folder: None,
            root,
            open: ctx.root_visible(root).unwrap_or(false),
            folders: Vec::new(),
            files: Vec::new(),
            folder_items: Vec::new(),
            folder_item_states: Vec::new(),
            file_items: Vec::new(),
            file_item_states: Vec::new(),
            folder_item_ids: Vec::new(),
            file_item_ids: Vec::new(),
            up_button,
            up_button_state,
            home_button,
            home_button_state,
            go_button,
            go_button_state,
            ok_button,
            ok_button_state,
            cancel_button,
            cancel_button_state,
            up_button_id: NodeId::default(),
            home_button_id: NodeId::default(),
            path_box_id: NodeId::default(),
            go_button_id: NodeId::default(),
            ok_button_id: NodeId::default(),
            cancel_button_id: NodeId::default(),
            folders_label: Self::projected_static_list_item(ListItemParameters::with_opt("Folders", WidgetOption::NO_INTERACT)),
            no_folders_label: Self::projected_static_list_item(ListItemParameters::with_opt("No folders", WidgetOption::NO_INTERACT)),
            files_label: Self::projected_static_list_item(ListItemParameters::with_opt("Files", WidgetOption::NO_INTERACT)),
            no_files_label: Self::projected_static_list_item(ListItemParameters::with_opt("No Files", WidgetOption::NO_INTERACT)),
            file_name_label: Self::projected_static_list_item(ListItemParameters::with_opt("File name:", WidgetOption::NO_INTERACT)),
            spacer_label: Self::projected_static_list_item(ListItemParameters::with_opt("", WidgetOption::NO_INTERACT)),
            tree: UiNodeSet::default(),
        };
        let _ = dialog
            .path_box_state
            .try_update_with(dialog.current_working_directory.clone(), |path_box, path| path_box.set_text(path));
        dialog.refresh_entries();
        dialog.sync_retained_root(ctx);
        dialog
    }

    /// Marks the dialog as open for the next frame.
    pub fn open<B: RendererBackend>(&mut self, ctx: &mut Context<B>) {
        ctx.set_root_visible(self.root, true);
        self.open = true;
    }

    /// Renders the dialog and updates the selected file when confirmed.
    pub fn eval<B: RendererBackend>(&mut self, ctx: &mut Context<B>) {
        let needs_refresh = self.apply_navigation_actions() || self.apply_folder_actions();
        self.apply_file_actions();
        let close = self.apply_completion_actions();
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
    use crate::test_support::{AllocationMeasurement, NoopRenderer, test_atlas};
    use std::{
        fs,
        time::{Instant, SystemTime, UNIX_EPOCH},
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
    fn action_buttons_keep_the_standard_control_height() {
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut ctx = Context::new_test(backend, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let toolbar = ctx
            .debug_root_node_rect(dialog.root, dialog.up_button_id)
            .expect("toolbar button should be laid out");
        let cancel = ctx
            .debug_root_node_rect(dialog.root, dialog.cancel_button_id)
            .expect("cancel button should be laid out");
        let open = ctx
            .debug_root_node_rect(dialog.root, dialog.ok_button_id)
            .expect("open button should be laid out");

        assert_eq!(cancel.height, toolbar.height);
        assert_eq!(open.height, toolbar.height);
    }

    #[test]
    fn browser_pane_absorbs_dialog_height_while_footer_stays_compact() {
        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut ctx = Context::new_test(backend, Dimensioni::new(900, 800));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let toolbar_before = ctx
            .debug_root_node_rect(dialog.root, dialog.up_button_id)
            .expect("toolbar button should be laid out");
        let open_before = ctx
            .debug_root_node_rect(dialog.root, dialog.ok_button_id)
            .expect("open button should be laid out");
        let body_before = ctx.debug_root_body(dialog.root).expect("dialog body should exist");
        let trailing_gap = body_before.y + body_before.height - (open_before.y + open_before.height);
        assert!(trailing_gap >= 0 && trailing_gap < toolbar_before.height);

        let mut resized = ctx.root_rect(dialog.root).expect("dialog root should exist");
        resized.height += 80;
        ctx.set_root_rect(dialog.root, resized);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let toolbar_after = ctx
            .debug_root_node_rect(dialog.root, dialog.up_button_id)
            .expect("toolbar button should remain laid out");
        let open_after = ctx
            .debug_root_node_rect(dialog.root, dialog.ok_button_id)
            .expect("open button should remain laid out");

        assert_eq!(
            (toolbar_after.x, toolbar_after.y, toolbar_after.width, toolbar_after.height),
            (toolbar_before.x, toolbar_before.y, toolbar_before.width, toolbar_before.height),
        );
        assert_eq!(open_after.height, open_before.height);
        assert_eq!(open_after.y - open_before.y, 80);
    }

    #[test]
    fn selecting_file_row_then_open_returns_selected_path() {
        let dir = unique_temp_dir("file-dialog-select");
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("picked.txt");
        fs::write(&file_path, b"picked").unwrap();

        let atlas = test_atlas();
        let backend = NoopRenderer { atlas };
        let mut ctx = Context::new_test(backend, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.current_working_directory = dir.to_string_lossy().to_string();
        dialog.refresh_entries();
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let file_node = dialog.file_item_ids[0];
        click_node(&mut ctx, dialog.root, file_node);
        dialog.eval(&mut ctx);

        assert_eq!(dialog.tmp_file_name_state.try_read(|tmp| tmp.text().to_string()).as_deref(), Some("picked.txt"));

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
        let backend = NoopRenderer { atlas };
        let mut ctx = Context::new_test(backend, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.current_working_directory = dir.to_string_lossy().to_string();
        dialog.refresh_entries();
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let file_node = dialog.file_item_ids[0];
        click_node_without_hover_frame(&mut ctx, dialog.root, file_node);
        dialog.eval(&mut ctx);

        assert_eq!(
            dialog.tmp_file_name_state.try_read(|tmp| tmp.text().to_string()).as_deref(),
            Some("batched.txt")
        );

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

    #[test]
    #[ignore = "manual serial release-mode P0/P5 UI-node baseline"]
    fn ui_node_p0_baseline_file_dialog() {
        let dir = unique_temp_dir("file-dialog-baseline");
        fs::create_dir_all(&dir).unwrap();
        for index in 0..8 {
            fs::write(dir.join(format!("file-{index}.txt")), b"baseline").unwrap();
        }
        for index in 0..4 {
            fs::create_dir(dir.join(format!("folder-{index}"))).unwrap();
        }

        let backend = NoopRenderer { atlas: test_atlas() };
        let mut ctx = Context::new_test(backend, Dimensioni::new(800, 600));
        let mut dialog = FileDialogState::new(&mut ctx);
        dialog.current_working_directory = dir.to_string_lossy().to_string();
        dialog.refresh_entries();
        dialog.open(&mut ctx);
        dialog.eval(&mut ctx);
        ctx.update_ui();
        dialog.eval(&mut ctx);
        ctx.update_ui();

        let replacements_before_idle = ctx.debug_root_projection_replacements();
        let idle_started = Instant::now();
        let idle_measurement = AllocationMeasurement::begin();
        dialog.eval(&mut ctx);
        ctx.update_ui();
        let idle_allocations = idle_measurement.finish();
        let idle_elapsed = idle_started.elapsed();
        let idle_replacements = ctx.debug_root_projection_replacements() - replacements_before_idle;
        let idle_metrics = ctx.debug_root_runtime_metrics(dialog.root).unwrap();
        let idle_structure = ctx.debug_root_structure(dialog.root).unwrap();

        fs::write(dir.join("new-file.txt"), b"refresh").unwrap();
        let replacements_before_refresh = ctx.debug_root_projection_replacements();
        let refresh_started = Instant::now();
        let refresh_measurement = AllocationMeasurement::begin();
        dialog.refresh_entries();
        dialog.eval(&mut ctx);
        ctx.update_ui();
        let refresh_allocations = refresh_measurement.finish();
        let refresh_elapsed = refresh_started.elapsed();
        let refresh_replacements = ctx.debug_root_projection_replacements() - replacements_before_refresh;
        let refresh_metrics = ctx.debug_root_runtime_metrics(dialog.root).unwrap();
        let refresh_structure = ctx.debug_root_structure(dialog.root).unwrap();

        println!(
            "| scenario | nodes | erased adapters | allocs | bytes | root rebuilds | tree layouts | measures | layouts | updates | paints | ns/eval+frame |"
        );
        println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
        println!(
            "| file dialog idle | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            idle_structure.0,
            idle_structure.1,
            idle_allocations.events,
            idle_allocations.bytes,
            idle_replacements,
            idle_metrics.tree_layouts,
            idle_metrics.measures,
            idle_metrics.layouts,
            idle_metrics.updates,
            idle_metrics.paints,
            idle_elapsed.as_nanos(),
        );
        println!(
            "| file dialog refresh | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            refresh_structure.0,
            refresh_structure.1,
            refresh_allocations.events,
            refresh_allocations.bytes,
            refresh_replacements,
            refresh_metrics.tree_layouts,
            refresh_metrics.measures,
            refresh_metrics.layouts,
            refresh_metrics.updates,
            refresh_metrics.paints,
            refresh_elapsed.as_nanos(),
        );

        assert_eq!(idle_replacements, 1);
        assert_eq!(refresh_replacements, 1);
        assert_eq!(idle_metrics.tree_layouts, 3);
        assert_eq!(refresh_metrics.tree_layouts, 3);
        assert!(idle_allocations.events > 0);
        assert!(refresh_allocations.events > idle_allocations.events);

        fs::remove_dir_all(dir).unwrap();
    }
}
