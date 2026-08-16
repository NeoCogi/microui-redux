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
//! Context-owned retained file picker with an application-polled session result.
//!
//! Opening a dialog constructs its retained shell once. Context advances navigation, selection,
//! and completion after each queued input event; applications only retain a [`FileDialogSession`]
//! and inspect [`FileDialogSession::status`] after [`crate::Context::update_ui`].
//!
//! Paths cross the public API as UTF-8 [`String`] values. On platforms that permit non-UTF-8 paths,
//! directory entries are converted lossily. Accepting a typed name is lexical: the dialog does not
//! require the resulting path to exist or to identify a regular file.

use std::{
    cell::RefCell,
    path::Path,
    rc::{Rc, Weak},
};

use crate::{
    Button, ButtonParameters, ButtonSubmitted, Column, ColumnParameters, IconId, LinearItem, ListItem, ListItemParameters, ListItemSubmitted, Node, Recti,
    RootHandle, RootSubmitted, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Textbox, TextboxParameters, TextboxSubmitted, ThemeIcons, TypedWidgetHandle,
    WidgetEventHandle, WidgetOption, WindowOption,
};
use crate::event::{WidgetEventListener, WidgetEventPort};
use crate::ui_node::RuntimeNodeId;
use crate::window_manager::WindowManager;

/// One-shot configuration used to open a file dialog.
#[derive(Clone, Debug)]
pub struct FileDialogRequest {
    title: String,
    initial_directory: String,
    rect: Recti,
}

impl FileDialogRequest {
    /// Creates a request with the default title, current process directory, and dialog rectangle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the title displayed by the retained dialog root.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Replaces the directory displayed when the dialog opens.
    ///
    /// A missing or unreadable directory remains visible in the path box and produces no filesystem
    /// entries; an available lexical parent can still be shown for navigation. Opening the dialog
    /// itself does not fail.
    pub fn with_initial_directory(mut self, directory: impl Into<String>) -> Self {
        self.initial_directory = directory.into();
        self
    }

    /// Replaces the initial outer rectangle in screen coordinates.
    pub const fn with_rect(mut self, rect: Recti) -> Self {
        self.rect = rect;
        self
    }

    /// Returns the configured dialog title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the configured initial directory.
    pub fn initial_directory(&self) -> &str {
        &self.initial_directory
    }

    /// Returns the configured initial outer rectangle.
    pub const fn rect(&self) -> Recti {
        self.rect
    }
}

impl Default for FileDialogRequest {
    fn default() -> Self {
        let initial_directory = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .into_owned();
        Self {
            title: "Open File".to_owned(),
            initial_directory,
            rect: Recti::new(50, 50, 720, 520),
        }
    }
}

/// Path accepted by a file dialog.
///
/// Acceptance does not imply that the path exists or identifies a regular file. Applications that
/// require those conditions must validate [`FileDialogResult::file_path`] after completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileDialogResult {
    /// Selected basename suitable for display.
    pub file_name: String,
    /// Selected path, resolved against the dialog's current directory when entered relatively.
    pub file_path: String,
}

/// Observable lifecycle state of a file-dialog session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDialogStatus {
    /// The retained dialog is still accepting input.
    Pending,
    /// The user accepted a non-empty file selection.
    Accepted(FileDialogResult),
    /// The user cancelled, closed, explicitly cancelled, or otherwise ended the dialog.
    Cancelled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct FileDialogSessionId(usize);

/// Read-only application capability for one Context-owned file dialog.
///
/// The session is deliberately not cloneable. Dropping it while pending abandons the dialog;
/// Context removes the retained root during its next UI update.
pub struct FileDialogSession {
    id: FileDialogSessionId,
    status: Rc<RefCell<FileDialogStatus>>,
}

impl FileDialogSession {
    /// Returns a repeatable owned snapshot of the current status.
    ///
    /// Terminal snapshots remain available after Context has removed the dialog root.
    pub fn status(&self) -> FileDialogStatus {
        self.status.borrow().clone()
    }
}

struct DialogRows {
    nodes: Vec<Node>,
    ids: Vec<RuntimeNodeId>,
}

enum ControllerDisposition {
    Pending,
    Remove,
}

pub(crate) struct FileDialogController {
    id: FileDialogSessionId,
    status: Weak<RefCell<FileDialogStatus>>,
    root: RootHandle,
    current_working_directory: String,
    folders: Vec<String>,
    files: Vec<String>,
    icons: ThemeIcons,
    folder_column: TypedWidgetHandle<Column>,
    file_column: TypedWidgetHandle<Column>,
    #[cfg_attr(not(test), allow(dead_code))]
    folder_scroll: TypedWidgetHandle<ScrollArea>,
    #[cfg_attr(not(test), allow(dead_code))]
    file_scroll: TypedWidgetHandle<ScrollArea>,
    folder_item_port: Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>,
    file_item_port: Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>,
    folder_item_events: WidgetEventListener<ListItemSubmitted>,
    file_item_events: WidgetEventListener<ListItemSubmitted>,
    folder_item_ids: Vec<RuntimeNodeId>,
    file_item_ids: Vec<RuntimeNodeId>,
    path_box: TypedWidgetHandle<Textbox>,
    path_box_submitted: WidgetEventListener<TextboxSubmitted>,
    file_name_box: TypedWidgetHandle<Textbox>,
    up_button: WidgetEventListener<ButtonSubmitted>,
    home_button: WidgetEventListener<ButtonSubmitted>,
    go_button: WidgetEventListener<ButtonSubmitted>,
    ok_button: WidgetEventListener<ButtonSubmitted>,
    cancel_button: WidgetEventListener<ButtonSubmitted>,
    root_submitted: WidgetEventListener<RootSubmitted>,
    #[cfg_attr(not(test), allow(dead_code))]
    up_button_id: RuntimeNodeId,
    #[cfg_attr(not(test), allow(dead_code))]
    ok_button_id: RuntimeNodeId,
    #[cfg_attr(not(test), allow(dead_code))]
    cancel_button_id: RuntimeNodeId,
}

impl Drop for FileDialogController {
    fn drop(&mut self) {
        let Some(status) = self.status.upgrade() else {
            return;
        };
        let mut status = status.borrow_mut();
        if matches!(*status, FileDialogStatus::Pending) {
            *status = FileDialogStatus::Cancelled;
        }
    }
}

impl FileDialogController {
    fn new(ctx: &mut WindowManager, id: FileDialogSessionId, status: Weak<RefCell<FileDialogStatus>>, request: FileDialogRequest) -> Self {
        let current_working_directory = request.initial_directory;
        let (folders, files) = Self::read_directory(Path::new(&current_working_directory));
        let icons = ctx.style().icons;
        let folder_item_port = Rc::new(RefCell::new(WidgetEventPort::new()));
        let file_item_port = Rc::new(RefCell::new(WidgetEventPort::new()));
        let folder_item_events = WidgetEventHandle::new(&folder_item_port).listen().unwrap();
        let file_item_events = WidgetEventHandle::new(&file_item_port).listen().unwrap();
        let folder_rows = Self::make_folder_rows(&current_working_directory, &folders, icons.closed_folder, &folder_item_port);
        let file_rows = Self::make_file_rows(&files, icons.file, &file_item_port);

        let (up_handle, up_node) = Button::create(ButtonParameters::new("Up"));
        let up_button = up_handle.submitted().listen().unwrap();
        let up_button_id = up_node.id();
        let (home_handle, home_node) = Button::create(ButtonParameters::new("Home"));
        let home_button = home_handle.submitted().listen().unwrap();
        let (path_box, path_node) = Textbox::create(TextboxParameters::new(current_working_directory.clone()));
        let path_box_submitted = path_box.submitted().listen().unwrap();
        let (go_handle, go_node) = Button::create(ButtonParameters::new("Go"));
        let go_button = go_handle.submitted().listen().unwrap();

        let folder_item_ids = folder_rows.ids;
        let file_item_ids = file_rows.ids;
        let (folder_column, folder_content) = Column::create(ColumnParameters::new(std::iter::once(Self::static_item("Folders")).chain(folder_rows.nodes)));
        let (file_column, file_content) = Column::create(ColumnParameters::new(std::iter::once(Self::static_item("Files")).chain(file_rows.nodes)));
        let (folder_scroll, folder_scroll_node) = ScrollArea::create(ScrollAreaParameters::new(
            ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
            folder_content,
        ));
        let (file_scroll, file_scroll_node) = ScrollArea::create(ScrollAreaParameters::new(
            ScrollAreaOption::FRAME | ScrollAreaOption::ENABLE_SCROLL,
            file_content,
        ));

        let (file_name_box, file_name_node) = Textbox::create(TextboxParameters::new(""));
        let (cancel_handle, cancel_node) = Button::create(ButtonParameters::new("Cancel"));
        let cancel_button = cancel_handle.submitted().listen().unwrap();
        let cancel_button_id = cancel_node.id();
        let (ok_handle, ok_node) = Button::create(ButtonParameters::new("Open"));
        let ok_button = ok_handle.submitted().listen().unwrap();
        let ok_button_id = ok_node.id();

        let (_, toolbar) = crate::Row::create(crate::RowParameters::new([
            LinearItem::fixed(up_node, 56),
            LinearItem::fixed(home_node, 56),
            LinearItem::flex(path_node, 1.0),
            LinearItem::fixed(go_node, 56),
        ]));
        let (_, browser) =
            crate::Row::create(crate::RowParameters::new([LinearItem::flex(folder_scroll_node, 1.0), LinearItem::flex(file_scroll_node, 2.0)]).fill_height());
        let (_, filename) = crate::Row::create(crate::RowParameters::new([
            LinearItem::fixed(Self::static_item("File name:"), 86),
            LinearItem::flex(file_name_node, 1.0),
        ]));
        let (_, actions) = crate::Row::create(crate::RowParameters::new([
            LinearItem::flex(Self::static_item(""), 1.0),
            LinearItem::fixed(cancel_node, 96),
            LinearItem::fixed(ok_node, 96),
        ]));
        let (_, shell) = Column::create(ColumnParameters::new([
            LinearItem::content(toolbar),
            LinearItem::flex(browser, 1.0),
            LinearItem::content(filename),
            LinearItem::content(actions),
        ]));

        let root = ctx.create_dialog(&request.title, request.rect, shell);
        ctx.set_root_options(root.id(), WindowOption::FRAME)
            .expect("new file-dialog root must accept options");
        ctx.set_root_visible(root.id(), true).expect("new file-dialog root must become visible");

        let root_submitted = root.submitted().listen().unwrap();

        Self {
            id,
            status,
            root,
            current_working_directory,
            folders,
            files,
            icons,
            folder_column,
            file_column,
            folder_scroll,
            file_scroll,
            folder_item_port,
            file_item_port,
            folder_item_events,
            file_item_events,
            folder_item_ids,
            file_item_ids,
            path_box,
            path_box_submitted,
            file_name_box,
            up_button,
            home_button,
            go_button,
            ok_button,
            cancel_button,
            root_submitted,
            up_button_id,
            ok_button_id,
            cancel_button_id,
        }
    }

    fn static_item(label: &str) -> Node {
        let (_, node) = ListItem::create(ListItemParameters::with_opt(label, WidgetOption::NO_INTERACT));
        node
    }

    fn read_directory(path: &Path) -> (Vec<String>, Vec<String>) {
        let mut folders = Vec::new();
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    folders.push(entry_path.to_string_lossy().into_owned());
                } else {
                    files.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        folders.sort();
        files.sort();
        if let Some(parent) = path.parent() {
            folders.insert(0, parent.to_string_lossy().into_owned());
        }
        (folders, files)
    }

    fn folder_label<'a>(cwd: &str, folder: &'a str) -> &'a str {
        let parent = Path::new(cwd).parent().map(|path| path.to_string_lossy().into_owned());
        if parent.as_deref() == Some(folder) {
            ".."
        } else {
            Path::new(folder).file_name().and_then(|name| name.to_str()).unwrap_or(folder)
        }
    }

    fn make_folder_rows(cwd: &str, folders: &[String], folder_icon: IconId, submitted_event: &Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>) -> DialogRows {
        if folders.is_empty() {
            return DialogRows {
                nodes: vec![Self::static_item("No folders")],
                ids: Vec::new(),
            };
        }
        let mut nodes = Vec::with_capacity(folders.len());
        let mut ids = Vec::with_capacity(folders.len());
        for folder in folders {
            let label = Self::folder_label(cwd, folder);
            let (_, node) = ListItem::create_with_event_port(ListItemParameters::with_icon(label, folder_icon), Rc::clone(submitted_event));
            ids.push(node.id());
            nodes.push(node);
        }
        DialogRows { nodes, ids }
    }

    fn make_file_rows(files: &[String], file_icon: IconId, submitted_event: &Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>) -> DialogRows {
        if files.is_empty() {
            return DialogRows {
                nodes: vec![Self::static_item("No files")],
                ids: Vec::new(),
            };
        }
        let mut nodes = Vec::with_capacity(files.len());
        let mut ids = Vec::with_capacity(files.len());
        for file in files {
            let (_, node) = ListItem::create_with_event_port(ListItemParameters::with_icon(file, file_icon), Rc::clone(submitted_event));
            ids.push(node.id());
            nodes.push(node);
        }
        DialogRows { nodes, ids }
    }

    fn refresh_entries(&mut self) {
        let (folders, files) = Self::read_directory(Path::new(&self.current_working_directory));
        let folder_rows = Self::make_folder_rows(&self.current_working_directory, &folders, self.icons.closed_folder, &self.folder_item_port);
        let file_rows = Self::make_file_rows(&files, self.icons.file, &self.file_item_port);
        let folder_ids = folder_rows.ids;
        let file_ids = file_rows.ids;
        if let Err(rejected) = replace_column_rows(
            &self.folder_column,
            std::iter::once(Self::static_item("Folders")).chain(folder_rows.nodes).collect(),
        ) {
            panic!("file-dialog folder column unavailable with {} replacement nodes", rejected.len());
        }
        if let Err(rejected) = replace_column_rows(&self.file_column, std::iter::once(Self::static_item("Files")).chain(file_rows.nodes).collect()) {
            panic!("file-dialog file column unavailable with {} replacement nodes", rejected.len());
        }

        self.folders = folders;
        self.files = files;
        self.folder_item_ids = folder_ids;
        self.file_item_ids = file_ids;
    }

    fn navigate_to(&mut self, directory: String) -> bool {
        if directory.is_empty() || directory == self.current_working_directory {
            return false;
        }
        self.current_working_directory = directory;
        self.path_box
            .try_update_with(self.current_working_directory.clone(), |state, path| state.set_text(path))
            .expect("file-dialog path box must remain mounted");
        self.file_name_box
            .try_update(Textbox::clear)
            .expect("file-dialog filename box must remain mounted");
        true
    }

    fn resolve_directory_path(&self, input: &str) -> Option<String> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }
        let raw = Path::new(input);
        let candidate = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            Path::new(&self.current_working_directory).join(raw)
        };
        candidate.is_dir().then(|| candidate.to_string_lossy().into_owned())
    }

    fn home_dir() -> Option<String> {
        std::env::var("HOME")
            .ok()
            .filter(|home| !home.is_empty())
            .or_else(|| std::env::var("USERPROFILE").ok().filter(|home| !home.is_empty()))
    }

    fn accepted_status(&self) -> Option<FileDialogStatus> {
        let typed_name = self.file_name_box.try_read(|state| state.text().trim().to_owned()).unwrap_or_default();
        if typed_name.is_empty() {
            return None;
        }
        let typed_path = Path::new(&typed_name);
        let file_path = if typed_path.is_absolute() {
            typed_path.to_string_lossy().into_owned()
        } else {
            Path::new(&self.current_working_directory).join(typed_path).to_string_lossy().into_owned()
        };
        let file_name = Path::new(&file_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or(typed_name);
        Some(FileDialogStatus::Accepted(FileDialogResult { file_name, file_path }))
    }

    fn navigate_and_refresh(&mut self, directory: String) {
        if self.navigate_to(directory) {
            self.refresh_entries();
        }
    }

    fn up_submitted(&mut self) {
        let parent = Path::new(&self.current_working_directory)
            .parent()
            .map(|path| path.to_string_lossy().into_owned());
        if let Some(parent) = parent {
            self.navigate_and_refresh(parent);
        }
    }

    fn home_submitted(&mut self) {
        if let Some(home) = Self::home_dir()
            && Path::new(&home).is_dir()
        {
            self.navigate_and_refresh(home);
        }
    }

    fn path_submitted(&mut self, event: &TextboxSubmitted) {
        if let Some(path) = self.resolve_directory_path(&event.text) {
            self.navigate_and_refresh(path);
        }
    }

    fn go_submitted(&mut self) {
        let input = self.path_box.try_read(|state| state.text().to_owned()).unwrap_or_default();
        if let Some(path) = self.resolve_directory_path(&input) {
            self.navigate_and_refresh(path);
        }
    }

    fn folder_submitted(&mut self, event: ListItemSubmitted) {
        let directory = self
            .folders
            .iter()
            .find(|folder| Self::folder_label(&self.current_working_directory, folder) == event.label)
            .cloned();
        if let Some(directory) = directory {
            self.navigate_and_refresh(directory);
        }
    }

    fn file_submitted(&mut self, event: ListItemSubmitted) {
        self.file_name_box
            .try_update_with(event.label, |state, name| state.set_text(name))
            .expect("file-dialog filename box must remain mounted");
    }

    fn complete(&mut self, completion: FileDialogStatus) {
        if let Some(status) = self.status.upgrade()
            && matches!(*status.borrow(), FileDialogStatus::Pending)
        {
            *status.borrow_mut() = completion;
        }
    }

    fn accept_submitted(&mut self) {
        if let Some(completion) = self.accepted_status() {
            self.complete(completion);
        }
    }

    fn cancel_submitted(&mut self) {
        self.complete(FileDialogStatus::Cancelled);
    }

    fn root_cancelled(&mut self) {
        self.complete(FileDialogStatus::Cancelled);
    }

    fn process(&mut self) -> ControllerDisposition {
        let Some(status) = self.status.upgrade() else {
            return ControllerDisposition::Remove;
        };
        if !matches!(*status.borrow(), FileDialogStatus::Pending) {
            return ControllerDisposition::Remove;
        }
        if !self.root.widget().is_alive() {
            *status.borrow_mut() = FileDialogStatus::Cancelled;
            return ControllerDisposition::Remove;
        }

        enum Action {
            Up,
            Home,
            Path(TextboxSubmitted),
            Go,
            Folder(ListItemSubmitted),
            File(ListItemSubmitted),
            Accept,
            Cancel,
            Root,
        }

        // Detach every pending native event before an action mutates the controller or replaces
        // dynamic row widgets and their listeners.
        let mut actions = Vec::new();
        actions.extend(self.up_button.drain().into_iter().map(|_| Action::Up));
        actions.extend(self.home_button.drain().into_iter().map(|_| Action::Home));
        actions.extend(self.path_box_submitted.drain().into_iter().map(Action::Path));
        actions.extend(self.go_button.drain().into_iter().map(|_| Action::Go));
        actions.extend(self.folder_item_events.drain().into_iter().map(Action::Folder));
        actions.extend(self.file_item_events.drain().into_iter().map(Action::File));
        actions.extend(self.ok_button.drain().into_iter().map(|_| Action::Accept));
        actions.extend(self.cancel_button.drain().into_iter().map(|_| Action::Cancel));
        actions.extend(self.root_submitted.drain().into_iter().map(|_| Action::Root));

        for action in actions {
            match action {
                Action::Up => self.up_submitted(),
                Action::Home => self.home_submitted(),
                Action::Path(event) => self.path_submitted(&event),
                Action::Go => self.go_submitted(),
                Action::Folder(directory) => self.folder_submitted(directory),
                Action::File(event) => self.file_submitted(event),
                Action::Accept => self.accept_submitted(),
                Action::Cancel => self.cancel_submitted(),
                Action::Root => self.root_cancelled(),
            }
        }
        if !matches!(*status.borrow(), FileDialogStatus::Pending) {
            return ControllerDisposition::Remove;
        }
        ControllerDisposition::Pending
    }

    fn belongs_to(&self, session: &FileDialogSession) -> bool {
        self.id == session.id && self.status.upgrade().is_some_and(|status| Rc::ptr_eq(&status, &session.status))
    }
}

#[allow(clippy::result_large_err)]
fn replace_column_rows(handle: &TypedWidgetHandle<Column>, nodes: Vec<Node>) -> Result<(), Vec<Node>> {
    handle.try_update_with(nodes, |state, nodes| state.replace(nodes))?
}

impl WindowManager {
    pub(crate) fn open_file_dialog(&mut self, request: FileDialogRequest) -> FileDialogSession {
        let id = FileDialogSessionId(self.next_file_dialog_id);
        self.next_file_dialog_id = self.next_file_dialog_id.checked_add(1).expect("file-dialog session id counter overflowed");
        let status = Rc::new(RefCell::new(FileDialogStatus::Pending));
        let controller = FileDialogController::new(self, id, Rc::downgrade(&status), request);
        self.file_dialogs.push(controller);
        FileDialogSession { id, status }
    }

    pub(crate) fn cancel_file_dialog(&mut self, session: &FileDialogSession) -> bool {
        let Some(index) = self.file_dialogs.iter().position(|dialog| dialog.belongs_to(session)) else {
            return false;
        };
        if !matches!(*session.status.borrow(), FileDialogStatus::Pending) {
            return false;
        }
        *session.status.borrow_mut() = FileDialogStatus::Cancelled;
        let dialog = self.file_dialogs.remove(index);
        let removed = self.destroy_root(dialog.root.id());
        debug_assert!(removed, "pending file-dialog controller must own a registered root");
        true
    }

    pub(crate) fn process_file_dialogs(&mut self) {
        let mut index = 0;
        while index < self.file_dialogs.len() {
            match self.file_dialogs[index].process() {
                ControllerDisposition::Pending => index += 1,
                ControllerDisposition::Remove => {
                    let dialog = self.file_dialogs.remove(index);
                    let _ = self.destroy_root(dialog.root.id());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{AllocationMeasurement, NoopRenderer, test_atlas};
    use crate::{Button, ButtonParameters, Context, Dimensioni, MouseButton, WindowOption, rect};
    use std::{
        fs,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("microui-redux-{name}-{}-{nanos}", std::process::id()))
    }

    fn context() -> Context<NoopRenderer> {
        Context::new_test(NoopRenderer { atlas: test_atlas() }, Dimensioni::new(900, 700))
    }

    fn controller<'a>(ctx: &'a Context<NoopRenderer>, session: &FileDialogSession) -> &'a FileDialogController {
        ctx.window_manager
            .file_dialogs
            .iter()
            .find(|dialog| dialog.belongs_to(session))
            .expect("pending session must have a controller")
    }

    fn click_node(ctx: &mut Context<NoopRenderer>, root: crate::RootId, node: RuntimeNodeId, batched: bool) {
        let rect = ctx.debug_root_node_rect(root, node).expect("node rect should be laid out");
        let x = rect.x + rect.width / 2;
        let y = rect.y + rect.height / 2;
        ctx.mousemove(x, y);
        if !batched {
            ctx.update_and_render_ui();
        }
        ctx.mousedown(x, y, MouseButton::LEFT);
        ctx.update_and_render_ui();
    }

    #[test]
    fn request_builders_and_pending_snapshot_are_public_contract() {
        let dir = unique_temp_dir("request");
        fs::create_dir_all(&dir).unwrap();
        let request = FileDialogRequest::new()
            .with_title("Choose")
            .with_initial_directory(dir.to_string_lossy())
            .with_rect(Recti::new(10, 20, 400, 300));
        assert_eq!(request.title(), "Choose");
        assert_eq!(request.initial_directory(), dir.to_string_lossy());
        let request_rect = request.rect();
        assert_eq!((request_rect.x, request_rect.y, request_rect.width, request_rect.height), (10, 20, 400, 300));

        let mut ctx = context();
        let session = ctx.open_file_dialog(request);
        assert_eq!(session.status(), FileDialogStatus::Pending);
        assert_eq!(session.status(), FileDialogStatus::Pending);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pending_file_dialog_blocks_pointer_input_to_underlying_windows() {
        let mut ctx = context();
        let (button, button_node) = Button::create(ButtonParameters::new("behind"));
        let submitted = button.submitted().listen().unwrap();
        let window = ctx.create_window("window", rect(0, 0, 100, 80), button_node);
        ctx.set_root_options(window.id(), WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        let dialog = controller(&ctx, &session).root.id();
        ctx.update_and_render_ui();

        assert_eq!(ctx.debug_modal_root(), Some(dialog));
        ctx.mousedown(10, 10, MouseButton::LEFT);
        ctx.mouseup(10, 10, MouseButton::LEFT);
        ctx.update_and_render_ui();

        assert!(submitted.drain().is_empty());
        assert_eq!(session.status(), FileDialogStatus::Pending);
        assert!(ctx.debug_root_zindex(dialog).unwrap() > ctx.debug_root_zindex(window.id()).unwrap());
    }

    #[test]
    fn action_buttons_keep_standard_height_and_browser_absorbs_resize() {
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        ctx.update_and_render_ui();
        let (root, up, cancel, open) = {
            let dialog = controller(&ctx, &session);
            (dialog.root.id(), dialog.up_button_id, dialog.cancel_button_id, dialog.ok_button_id)
        };
        let toolbar_before = ctx.debug_root_node_rect(root, up).unwrap();
        let cancel_before = ctx.debug_root_node_rect(root, cancel).unwrap();
        let open_before = ctx.debug_root_node_rect(root, open).unwrap();
        assert_eq!(cancel_before.height, toolbar_before.height);
        assert_eq!(open_before.height, toolbar_before.height);

        let body_before = ctx.debug_root_body(root).unwrap();
        let trailing_gap = body_before.y + body_before.height - (open_before.y + open_before.height);
        assert!(trailing_gap >= 0 && trailing_gap < toolbar_before.height);
        let mut resized = controller(&ctx, &session).root.widget().try_read(crate::RootChrome::rect).unwrap();
        resized.height += 80;
        ctx.set_root_rect(root, resized).unwrap();
        ctx.update_and_render_ui();
        let toolbar_after = ctx.debug_root_node_rect(root, up).unwrap();
        let open_after = ctx.debug_root_node_rect(root, open).unwrap();
        assert_eq!(
            (toolbar_after.x, toolbar_after.y, toolbar_after.width, toolbar_after.height),
            (toolbar_before.x, toolbar_before.y, toolbar_before.width, toolbar_before.height)
        );
        assert_eq!(open_after.height, open_before.height);
        assert_eq!(open_after.y - open_before.y, 80);
    }

    fn selection_flow(batched: bool) {
        let dir = unique_temp_dir(if batched { "batched" } else { "select" });
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("picked.txt");
        fs::write(&file_path, b"picked").unwrap();
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        ctx.update_and_render_ui();
        let (root, file_node) = {
            let dialog = controller(&ctx, &session);
            (dialog.root.id(), dialog.file_item_ids[0])
        };
        click_node(&mut ctx, root, file_node, batched);
        assert_eq!(
            controller(&ctx, &session).file_name_box.try_read(|state| state.text().to_owned()).as_deref(),
            Some("picked.txt")
        );
        ctx.mouseup(0, 0, MouseButton::LEFT);
        ctx.update_and_render_ui();
        let open = controller(&ctx, &session).ok_button_id;
        let open_rect = ctx.debug_root_node_rect(root, open).unwrap();
        click_node(&mut ctx, root, open, batched);
        let expected = FileDialogStatus::Accepted(FileDialogResult {
            file_name: "picked.txt".to_owned(),
            file_path: file_path.to_string_lossy().into_owned(),
        });
        assert_eq!(
            session.status(),
            expected,
            "open rect=({}, {}, {}, {}), filename={:?}",
            open_rect.x,
            open_rect.y,
            open_rect.width,
            open_rect.height,
            ctx.window_manager
                .file_dialogs
                .first()
                .and_then(|dialog| dialog.file_name_box.try_read(|state| state.text().to_owned()))
        );
        assert_eq!(session.status(), expected);
        assert!(ctx.window_manager.file_dialogs.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selecting_file_then_open_accepts_and_removes_root() {
        selection_flow(false);
    }

    #[test]
    fn clicks_work_when_move_and_press_are_queued_together() {
        selection_flow(true);
    }

    #[test]
    fn empty_accept_stays_pending_and_cancel_button_terminates() {
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        ctx.update_and_render_ui();
        let (root, open, cancel, root_widget) = {
            let dialog = controller(&ctx, &session);
            (dialog.root.id(), dialog.ok_button_id, dialog.cancel_button_id, dialog.root.widget().clone())
        };
        click_node(&mut ctx, root, open, false);
        assert_eq!(session.status(), FileDialogStatus::Pending);
        assert!(root_widget.is_alive());

        ctx.mouseup(0, 0, MouseButton::LEFT);
        ctx.update_and_render_ui();
        click_node(&mut ctx, root, cancel, false);
        assert_eq!(session.status(), FileDialogStatus::Cancelled);
        assert!(!root_widget.is_alive());
    }

    #[test]
    fn title_close_cancels_and_removes_the_dialog_root() {
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        ctx.update_and_render_ui();
        let (root, root_widget) = {
            let dialog = controller(&ctx, &session);
            (dialog.root.id(), dialog.root.widget().clone())
        };
        let close = ctx.debug_root_chrome(root).unwrap().1.expect("dialog should have a close button");
        let x = close.x + close.width / 2;
        let y = close.y + close.height / 2;
        ctx.mousemove(x, y);
        ctx.mousedown(x, y, MouseButton::LEFT);
        ctx.update_and_render_ui();
        assert_eq!(session.status(), FileDialogStatus::Cancelled);
        assert!(!root_widget.is_alive());
    }

    #[test]
    fn folder_submission_navigates_and_replaces_only_dynamic_rows() {
        let dir = unique_temp_dir("navigate");
        let child = dir.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("inside.txt"), b"inside").unwrap();
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        ctx.update_and_render_ui();
        let (root, child_node, folder_scroll, path_box) = {
            let dialog = controller(&ctx, &session);
            let index = dialog
                .folders
                .iter()
                .position(|folder| Path::new(folder) == child)
                .expect("child directory should be listed");
            (
                dialog.root.id(),
                dialog.folder_item_ids[index],
                dialog.folder_scroll.clone(),
                dialog.path_box.clone(),
            )
        };
        click_node(&mut ctx, root, child_node, false);
        let dialog = controller(&ctx, &session);
        assert_eq!(Path::new(&dialog.current_working_directory), child);
        assert_eq!(dialog.files, ["inside.txt"]);
        assert!(folder_scroll.is_alive());
        assert!(path_box.is_alive());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_cancel_is_context_checked_and_terminal_status_is_stable() {
        let mut owner = context();
        let mut foreign = context();
        let session = owner.open_file_dialog(FileDialogRequest::default());
        let root_widget = controller(&owner, &session).root.widget().clone();
        assert!(!foreign.cancel_file_dialog(&session));
        assert!(owner.cancel_file_dialog(&session));
        assert!(!owner.cancel_file_dialog(&session));
        assert_eq!(session.status(), FileDialogStatus::Cancelled);
        assert_eq!(session.status(), FileDialogStatus::Cancelled);
        assert!(!root_widget.is_alive());
    }

    #[test]
    fn dropping_pending_session_removes_root_on_next_update() {
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        let root_widget = controller(&ctx, &session).root.widget().clone();
        drop(session);
        assert!(root_widget.is_alive());
        ctx.update_ui(Dimensioni::new(900, 700));
        assert!(!root_widget.is_alive());
        assert!(ctx.window_manager.file_dialogs.is_empty());
    }

    #[test]
    fn dropping_context_cancels_a_still_observed_session() {
        let session = {
            let mut ctx = context();
            ctx.open_file_dialog(FileDialogRequest::default())
        };
        assert_eq!(session.status(), FileDialogStatus::Cancelled);
    }

    #[test]
    fn refresh_replaces_only_model_data_and_preserves_then_clamps_scroll() {
        let dir = unique_temp_dir("refresh");
        fs::create_dir_all(&dir).unwrap();
        for index in 0..60 {
            fs::write(dir.join(format!("file-{index:02}.txt")), b"row").unwrap();
        }
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        ctx.update_ui(Dimensioni::new(900, 700));
        let (path, folder_scroll, file_scroll, root, shell_count) = {
            let dialog = controller(&ctx, &session);
            (
                dialog.path_box.clone(),
                dialog.folder_scroll.clone(),
                dialog.file_scroll.clone(),
                dialog.root.id(),
                ctx.debug_root_node_count(dialog.root.id()).unwrap(),
            )
        };
        file_scroll.try_update(|state| state.set_offset(crate::vec2(0, 40))).unwrap();
        fs::write(dir.join("new-file.txt"), b"row").unwrap();
        ctx.window_manager.file_dialogs[0].refresh_entries();
        assert!(path.is_alive());
        assert!(folder_scroll.is_alive());
        assert!(file_scroll.is_alive());
        ctx.update_ui(Dimensioni::new(900, 700));
        assert_eq!(file_scroll.try_read(|state| state.offset().y), Some(40));
        assert_eq!(ctx.debug_root_node_count(root), Some(shell_count + 1));

        for entry in fs::read_dir(&dir).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        ctx.window_manager.file_dialogs[0].refresh_entries();
        ctx.update_ui(Dimensioni::new(900, 700));
        assert_eq!(file_scroll.try_read(|state| state.offset().y), Some(0));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unavailable_column_replacement_returns_every_node() {
        let (column, owner) = Column::create(ColumnParameters::default());
        let rejected = column
            .try_read(|_| {
                let replacements = vec![FileDialogController::static_item("one"), FileDialogController::static_item("two")];
                replace_column_rows(&column, replacements).expect_err("active read must reject mutation")
            })
            .unwrap();
        assert_eq!(rejected.len(), 2);
        drop(owner);
    }

    #[test]
    fn idle_processing_allocates_no_nodes_or_state_and_changes_no_topology() {
        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::default());
        ctx.update_ui(Dimensioni::new(900, 700));
        let root = controller(&ctx, &session).root.id();
        let node_count = ctx.debug_root_node_count(root).unwrap();
        let measurement = AllocationMeasurement::begin();
        ctx.window_manager.process_file_dialogs();
        let allocations = measurement.finish();
        assert_eq!(allocations.events, 0);
        assert_eq!(ctx.debug_root_node_count(root), Some(node_count));
        assert_eq!(session.status(), FileDialogStatus::Pending);
    }

    #[test]
    #[ignore = "manual serial release-mode P5.1 retained file-dialog baseline"]
    fn ui_node_p5_baseline_file_dialog() {
        let dir = unique_temp_dir("p5-baseline");
        fs::create_dir_all(&dir).unwrap();
        for index in 0..300 {
            fs::write(dir.join(format!("file-{index}.txt")), b"baseline").unwrap();
        }
        for index in 0..4 {
            fs::create_dir(dir.join(format!("folder-{index}"))).unwrap();
        }

        let mut ctx = context();
        let session = ctx.open_file_dialog(FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        ctx.update_and_render_ui();
        ctx.update_and_render_ui();

        let (root, node_count, persistent_scroll, persistent_path) = {
            let dialog = controller(&ctx, &session);
            (
                dialog.root.id(),
                ctx.debug_root_node_count(dialog.root.id()).unwrap(),
                dialog.file_scroll.clone(),
                dialog.path_box.clone(),
            )
        };

        // The controller itself must perform no hidden state allocation or topology work while
        // pending and idle. The full UI row below separately records layout and paint allocations.
        let controller_measurement = AllocationMeasurement::begin();
        ctx.window_manager.process_file_dialogs();
        let controller_allocations = controller_measurement.finish();
        assert_eq!(controller_allocations.events, 0);
        assert_eq!(ctx.debug_root_node_count(root), Some(node_count));

        let idle_measurement = AllocationMeasurement::begin();
        let idle_started = Instant::now();
        ctx.update_and_render_ui();
        let idle_elapsed = idle_started.elapsed();
        let idle_allocations = idle_measurement.finish();
        let idle_metrics = ctx.debug_root_runtime_metrics(root).unwrap();
        assert_eq!(idle_metrics.tree_layouts, 1);
        assert_eq!(idle_metrics.updates, 0);
        assert!(idle_metrics.paints <= node_count as u64);
        assert_eq!(ctx.debug_root_node_count(root), Some(node_count));

        ctx.mousemove(100, 100);
        let event_measurement = AllocationMeasurement::begin();
        let event_started = Instant::now();
        ctx.update_and_render_ui();
        let event_elapsed = event_started.elapsed();
        let event_allocations = event_measurement.finish();
        let event_metrics = ctx.debug_root_runtime_metrics(root).unwrap();
        assert_eq!(event_metrics.tree_layouts, 2);
        assert_eq!(event_metrics.updates, event_metrics.paints);
        assert_eq!(ctx.debug_root_node_count(root), Some(node_count));

        fs::write(dir.join("new-file.txt"), b"refresh").unwrap();
        let refresh_measurement = AllocationMeasurement::begin();
        let refresh_started = Instant::now();
        ctx.window_manager.file_dialogs[0].refresh_entries();
        ctx.update_and_render_ui();
        let refresh_elapsed = refresh_started.elapsed();
        let refresh_allocations = refresh_measurement.finish();
        let refresh_metrics = ctx.debug_root_runtime_metrics(root).unwrap();
        let refresh_node_count = ctx.debug_root_node_count(root).unwrap();

        assert_eq!(refresh_node_count, node_count + 1);
        assert_eq!(refresh_metrics.tree_layouts, 1);
        assert_eq!(refresh_metrics.updates, 0);
        assert!(refresh_metrics.paints <= refresh_node_count as u64);
        assert!(persistent_scroll.is_alive());
        assert!(persistent_path.is_alive());
        assert_eq!(controller(&ctx, &session).root.id(), root, "refresh must retain the root");
        assert_eq!(session.status(), FileDialogStatus::Pending);

        println!("| scenario | total retained nodes | allocs | bytes | root rebuilds | tree layouts | measures | layouts | updates | paints | ns/operation |");
        println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
        println!(
            "| file dialog idle | {} | {} | {} | 0 | {} | {} | {} | {} | {} | {} |",
            node_count,
            idle_allocations.events,
            idle_allocations.bytes,
            idle_metrics.tree_layouts,
            idle_metrics.measures,
            idle_metrics.layouts,
            idle_metrics.updates,
            idle_metrics.paints,
            idle_elapsed.as_nanos(),
        );
        println!(
            "| file dialog mouse move | {} | {} | {} | 0 | {} | {} | {} | {} | {} | {} |",
            node_count,
            event_allocations.events,
            event_allocations.bytes,
            event_metrics.tree_layouts,
            event_metrics.measures,
            event_metrics.layouts,
            event_metrics.updates,
            event_metrics.paints,
            event_elapsed.as_nanos(),
        );
        println!(
            "| file dialog refresh | {} | {} | {} | 0 | {} | {} | {} | {} | {} | {} |",
            refresh_node_count,
            refresh_allocations.events,
            refresh_allocations.bytes,
            refresh_metrics.tree_layouts,
            refresh_metrics.measures,
            refresh_metrics.layouts,
            refresh_metrics.updates,
            refresh_metrics.paints,
            refresh_elapsed.as_nanos(),
        );

        drop(ctx);
        drop(session);
        fs::remove_dir_all(dir).unwrap();
    }
}
