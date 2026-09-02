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
//! Application-owned retained file picker with typed completion delivery.
//!
//! [`FileDialog`] is a library component, not a window-manager service. An application constructs
//! and stores one alongside its own state. The component registers a hidden dialog window
//! as a stable child of an application window and binds its controls to the application's normal
//! [`crate::Context`] dispatcher. Opening resets and shows that window; acceptance or cancellation
//! hides it and emits [`FileDialogCompleted`] from the component-owned source.
//!
//! Paths cross the public API as UTF-8 [`String`] values. On platforms that permit non-UTF-8 paths,
//! directory entries are converted lossily. Accepting a typed name is lexical: the dialog does not
//! require the resulting path to exist or to identify a regular file.

use std::{cell::RefCell, path::Path, rc::Rc};

use crate::{
    Button, ButtonParameters, ButtonSubmitted, Context, IconRef, IconRole, Linear, LinearItem, LinearParameters, ListItem, ListItemParameters,
    ListItemSubmitted, Node, Recti, ScrollArea, ScrollAreaOption, ScrollAreaParameters, Textbox, TextboxParameters, TextboxSubmitted, TypedWidgetHandle, Ui,
    WidgetEventPortHandle, WidgetOption, Window, WindowEvent, WindowHandle, WindowOption,
};
use crate::event::WidgetEventPort;
#[cfg(test)]
use crate::ui_node::RuntimeNodeId;

const DEFAULT_FILE_DIALOG_TITLE: &str = "Open File";
const DEFAULT_FILE_DIALOG_RECT: Recti = Recti { x: 50, y: 50, width: 720, height: 520 };

/// One-shot configuration used to open a file dialog.
///
/// # Path encoding
///
/// File-dialog paths cross the public API as UTF-8 [`String`] values. On platforms that permit
/// non-UTF-8 paths, the default current directory and directory entries encountered while browsing
/// are converted lossily.
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

    /// Replaces the title displayed by the retained dialog window.
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
            title: DEFAULT_FILE_DIALOG_TITLE.to_owned(),
            initial_directory,
            rect: DEFAULT_FILE_DIALOG_RECT,
        }
    }
}

/// Path accepted by a file dialog.
///
/// `file_name` and `file_path` are UTF-8 [`String`] values. On platforms that permit non-UTF-8
/// paths, filesystem entries encountered by the dialog are converted lossily. Acceptance does not
/// imply that the path exists or identifies a regular file; applications that require those
/// conditions must validate [`FileDialogResult::file_path`] after completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileDialogResult {
    /// Selected basename suitable for display.
    pub file_name: String,
    /// Selected path, resolved against the dialog's current directory when entered relatively.
    pub file_path: String,
}

/// Terminal outcome of one file-dialog activation.
///
/// Completion events always carry one of these outcomes; pending state is represented by
/// [`FileDialog::is_open`] rather than an impossible event variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileDialogStatus {
    /// The user accepted a non-empty file selection.
    Accepted(FileDialogResult),
    /// The user cancelled, closed, explicitly cancelled, or otherwise ended the dialog.
    Cancelled,
}

/// One terminal transition emitted by an application-owned [`FileDialog`].
///
/// Each dialog owns a distinct source returned by [`FileDialog::completed`], so the source identity
/// itself identifies the component that completed.
#[derive(Clone, Debug)]
pub struct FileDialogCompleted {
    /// Owned terminal snapshot produced before the dialog returns to hidden idle state.
    status: FileDialogStatus,
}

impl FileDialogCompleted {
    fn new(status: FileDialogStatus) -> Self {
        Self { status }
    }

    /// Returns the terminal completion snapshot carried by this event.
    pub fn status(&self) -> &FileDialogStatus {
        // Borrow the event-owned value so handlers can inspect results without another clone.
        &self.status
    }
}

impl crate::WidgetEvent for FileDialogCompleted {}

/// Newly built dynamic rows transferred into one retained dialog column.
struct DialogRows {
    /// Unique row nodes awaiting transfer into the owning `Linear` container.
    nodes: Vec<Node>,
    /// Runtime identities paired with `nodes` for test-only geometry lookup.
    #[cfg(test)]
    ids: Vec<RuntimeNodeId>,
}

/// Test-only identities for static and dynamic dialog controls.
#[cfg(test)]
struct FileDialogTestFields {
    /// Current folder-row identities in display order.
    folder_item_ids: Vec<RuntimeNodeId>,
    /// Current file-row identities in display order.
    file_item_ids: Vec<RuntimeNodeId>,
    /// Persistent toolbar Up button identity.
    up_button_id: RuntimeNodeId,
    /// Persistent acceptance button identity.
    ok_button_id: RuntimeNodeId,
    /// Persistent cancellation button identity.
    cancel_button_id: RuntimeNodeId,
}

/// Function pointer used by retained subscriptions to recover one application-owned dialog.
type FileDialogAccessor<State> = for<'a> fn(&'a mut State) -> &'a mut FileDialog;

/// Reusable retained file-picker component owned by application state.
///
/// Construct it once with [`FileDialog::new`], store the returned value at the location selected by
/// the supplied accessor, and subscribe to [`FileDialog::completed`] with the application's normal
/// `Context` dispatcher. The window manager sees one hidden modal window owned by the caller-supplied
/// application window; all file-picker behavior remains in this component.
pub struct FileDialog {
    /// Application-side semantic activation, synchronized with the hidden dialog window.
    active: bool,
    /// Component-owned terminal event source retained independently of any one activation.
    completed: Rc<RefCell<WidgetEventPort<FileDialogCompleted>>>,
    /// Non-owning stable capability and event projection for the Context-owned dialog window.
    window: WindowHandle,
    /// UTF-8 directory currently represented by the two retained list columns.
    current_working_directory: String,
    /// Display names currently mounted in the folder column.
    folders: Vec<String>,
    /// Display names currently mounted in the file column.
    files: Vec<String>,
    /// Weak retained topology capability for replacing folder rows in place.
    folder_column: TypedWidgetHandle<Linear>,
    /// Weak retained topology capability for replacing file rows in place.
    file_column: TypedWidgetHandle<Linear>,
    /// Retained folder viewport state used by refresh and tests.
    #[cfg_attr(not(test), allow(dead_code))]
    folder_scroll: TypedWidgetHandle<ScrollArea>,
    /// Retained file viewport state used by refresh and tests.
    #[cfg_attr(not(test), allow(dead_code))]
    file_scroll: TypedWidgetHandle<ScrollArea>,
    /// Shared typed source used by every dynamically rebuilt folder row.
    folder_item_port: Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>,
    /// Shared typed source used by every dynamically rebuilt file row.
    file_item_port: Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>,
    /// Weak handle for the editable directory path.
    path_box: TypedWidgetHandle<Textbox>,
    /// Weak handle for the accepted file name.
    file_name_box: TypedWidgetHandle<Textbox>,
    /// Node identities used only for retained geometry and interaction tests.
    #[cfg(test)]
    test: FileDialogTestFields,
}

impl FileDialog {
    /// Builds one hidden retained file picker and binds its controls to application state.
    ///
    /// `parent` is the stable application window that owns the dialog. Destroying that window also
    /// destroys the dialog, while hiding it temporarily hides the dialog without discarding its
    /// retained controls. Ownership does not make dialog geometry parent-relative.
    ///
    /// The accessor is stored with the component's subscriptions and is not invoked during
    /// construction. Once the returned value is placed in application state, the accessor must
    /// resolve that same `FileDialog` for the remainder of the component's lifetime.
    pub fn new<B: crate::render::RendererBackend, State: 'static>(
        ctx: &mut Context<B, State>,
        parent: &WindowHandle,
        accessor: for<'a> fn(&'a mut State) -> &'a mut FileDialog,
    ) -> Self {
        // Start with an empty inactive model. The first activation supplies directory data, title, and
        // geometry immediately before this already-retained modal window becomes visible.
        let current_working_directory = String::new();
        let folders = Vec::new();
        let files = Vec::new();
        let folder_item_port = Rc::new(RefCell::new(WidgetEventPort::new()));
        let file_item_port = Rc::new(RefCell::new(WidgetEventPort::new()));
        let folder_rows = Self::make_folder_rows(&current_working_directory, &folders, &folder_item_port);
        let file_rows = Self::make_file_rows(&files, &file_item_port);

        let (up_handle, up_node) = Button::create(ButtonParameters::new("Up"));
        let (home_handle, home_node) = Button::create(ButtonParameters::new("Home"));
        let (path_box, path_node) = Textbox::create(TextboxParameters::new(current_working_directory.clone()));
        let (go_handle, go_node) = Button::create(ButtonParameters::new("Go"));

        let (folder_column, folder_content) = Linear::create(LinearParameters::vertical(
            std::iter::once(Self::static_item("Folders")).chain(folder_rows.nodes),
        ));
        let (file_column, file_content) = Linear::create(LinearParameters::vertical(std::iter::once(Self::static_item("Files")).chain(file_rows.nodes)));
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
        let (ok_handle, ok_node) = Button::create(ButtonParameters::new("Open"));

        #[cfg(test)]
        let test = FileDialogTestFields {
            folder_item_ids: folder_rows.ids,
            file_item_ids: file_rows.ids,
            up_button_id: up_node.id(),
            ok_button_id: ok_node.id(),
            cancel_button_id: cancel_node.id(),
        };

        let (_, toolbar) = Linear::create(LinearParameters::horizontal([
            LinearItem::fixed(up_node, 56),
            LinearItem::fixed(home_node, 56),
            LinearItem::flex(path_node, 1.0),
            LinearItem::fixed(go_node, 56),
        ]));
        let (_, browser) =
            Linear::create(LinearParameters::horizontal([LinearItem::flex(folder_scroll_node, 1.0), LinearItem::flex(file_scroll_node, 2.0)]).stretch_cross());
        let (_, filename) = Linear::create(LinearParameters::horizontal([
            LinearItem::fixed(Self::static_item("File name:"), 86),
            LinearItem::flex(file_name_node, 1.0),
        ]));
        let (_, actions) = Linear::create(LinearParameters::horizontal([
            LinearItem::flex(Self::static_item(""), 1.0),
            LinearItem::fixed(cancel_node, 96),
            LinearItem::fixed(ok_node, 96),
        ]));
        let (_, shell) = Linear::create(LinearParameters::vertical([
            LinearItem::content(toolbar),
            LinearItem::flex(browser, 1.0),
            LinearItem::content(filename),
            LinearItem::content(actions),
        ]));

        // Register the modal surface as a stable child of the application window supplied by the
        // caller. Construction fails only when that typed parent capability is stale or ineligible.
        let window = ctx
            .ui()
            .create_dialog(parent, Window::new(DEFAULT_FILE_DIALOG_TITLE, DEFAULT_FILE_DIALOG_RECT, shell))
            .expect("new file-dialog parent must remain registered");
        ctx.ui()
            .set_window_options(&window, WindowOption::FRAME)
            .expect("new file-dialog window must accept options");

        // These sources are new and private to this component, so connection failure would indicate
        // an internal construction error rather than an application-level subscription conflict.
        ctx.subscribe_with(up_handle.submitted(), accessor, Self::dispatch_up::<State>)
            .expect("new file-dialog Up event must be unsubscribed");
        ctx.subscribe_with(home_handle.submitted(), accessor, Self::dispatch_home::<State>)
            .expect("new file-dialog Home event must be unsubscribed");
        ctx.subscribe_with(path_box.submitted(), accessor, Self::dispatch_path::<State>)
            .expect("new file-dialog path event must be unsubscribed");
        ctx.subscribe_with(go_handle.submitted(), accessor, Self::dispatch_go::<State>)
            .expect("new file-dialog Go event must be unsubscribed");
        ctx.subscribe_with(WidgetEventPortHandle::new(&folder_item_port), accessor, Self::dispatch_folder::<State>)
            .expect("new file-dialog folder event must be unsubscribed");
        ctx.subscribe_with(WidgetEventPortHandle::new(&file_item_port), accessor, Self::dispatch_file::<State>)
            .expect("new file-dialog file event must be unsubscribed");
        ctx.subscribe_context_with(ok_handle.submitted(), accessor, Self::dispatch_accept::<State>)
            .expect("new file-dialog Open event must be unsubscribed");
        ctx.subscribe_context_with(cancel_handle.submitted(), accessor, Self::dispatch_cancel::<State>)
            .expect("new file-dialog Cancel event must be unsubscribed");
        ctx.subscribe_context_with(window.events(), accessor, Self::dispatch_window_event::<State>)
            .expect("new file-dialog window event must be unsubscribed");

        Self {
            active: false,
            completed: Rc::new(RefCell::new(WidgetEventPort::new())),
            window,
            current_working_directory,
            folders,
            files,
            folder_column,
            file_column,
            folder_scroll,
            file_scroll,
            folder_item_port,
            file_item_port,
            path_box,
            file_name_box,
            #[cfg(test)]
            test,
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

    fn make_folder_rows(cwd: &str, folders: &[String], submitted_event: &Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>) -> DialogRows {
        if folders.is_empty() {
            return DialogRows {
                nodes: vec![Self::static_item("No folders")],
                #[cfg(test)]
                ids: Vec::new(),
            };
        }
        let mut nodes = Vec::with_capacity(folders.len());
        #[cfg(test)]
        let mut ids = Vec::with_capacity(folders.len());
        for folder in folders {
            let label = Self::folder_label(cwd, folder);
            // Rows retain the semantic role, so an open dialog adopts replacement skin artwork
            // without rebuilding its retained topology or keeping an old atlas capability alive.
            let (_, node) = ListItem::create_with_event_port(
                ListItemParameters::with_icon(label, IconRef::role(IconRole::ClosedFolder)),
                Rc::clone(submitted_event),
            );
            #[cfg(test)]
            ids.push(node.id());
            nodes.push(node);
        }
        DialogRows {
            nodes,
            #[cfg(test)]
            ids,
        }
    }

    fn make_file_rows(files: &[String], submitted_event: &Rc<RefCell<WidgetEventPort<ListItemSubmitted>>>) -> DialogRows {
        if files.is_empty() {
            return DialogRows {
                nodes: vec![Self::static_item("No files")],
                #[cfg(test)]
                ids: Vec::new(),
            };
        }
        let mut nodes = Vec::with_capacity(files.len());
        #[cfg(test)]
        let mut ids = Vec::with_capacity(files.len());
        for file in files {
            let (_, node) = ListItem::create_with_event_port(ListItemParameters::with_icon(file, IconRef::role(IconRole::File)), Rc::clone(submitted_event));
            #[cfg(test)]
            ids.push(node.id());
            nodes.push(node);
        }
        DialogRows {
            nodes,
            #[cfg(test)]
            ids,
        }
    }

    fn refresh_entries(&mut self) {
        let (folders, files) = Self::read_directory(Path::new(&self.current_working_directory));
        let folder_rows = Self::make_folder_rows(&self.current_working_directory, &folders, &self.folder_item_port);
        let file_rows = Self::make_file_rows(&files, &self.file_item_port);
        #[cfg(test)]
        let folder_item_ids = folder_rows.ids;
        #[cfg(test)]
        let file_item_ids = file_rows.ids;
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
        #[cfg(test)]
        {
            self.test.folder_item_ids = folder_item_ids;
            self.test.file_item_ids = file_item_ids;
        }
    }

    /// Returns the completion source owned by this component.
    pub fn completed(&self) -> WidgetEventPortHandle<FileDialogCompleted> {
        WidgetEventPortHandle::new(&self.completed)
    }

    /// Returns the retained dialog window used by this component.
    ///
    /// Its typed handle supports the same checked mutations and event access as any other window.
    /// Keep lifecycle changes routed through
    /// [`Self::open`] or [`Self::cancel`] so component activity and modal visibility stay
    /// synchronized.
    pub fn window(&self) -> &WindowHandle {
        // Lend the aggregate capability without transferring ownership or exposing its private ID.
        &self.window
    }

    /// Returns whether this component is open, regardless of which visible dialog is frontmost.
    pub const fn is_open(&self) -> bool {
        self.active
    }

    /// Resets and shows this file picker through the current retained UI transaction.
    ///
    /// Completion is emitted through [`Self::completed`] during a later retained event dispatch.
    ///
    /// # Panics
    ///
    /// Panics when the dialog is already open or its application-owned window was destroyed.
    pub fn open(&mut self, ui: &mut Ui<'_>, request: FileDialogRequest) {
        let (title, rect) = self.prepare_open(request);
        ui.set_window_name(&self.window, title)
            .expect("application-owned file-dialog window must remain registered");
        ui.set_window_rect(&self.window, rect)
            .expect("application-owned file-dialog window must remain registered");
        ui.set_window_visible(&self.window, true)
            .expect("application-owned file-dialog window must remain registered");
    }

    /// Cancels an open picker through the current retained UI transaction.
    ///
    /// Returns `false` when it is already idle. A successful cancellation queues exactly one
    /// [`FileDialogCompleted`] event for the next application dispatch boundary.
    pub fn cancel(&mut self, ui: &mut Ui<'_>) -> bool {
        if !self.active {
            return false;
        }
        // Reuse the same completion path as buttons and title-bar dismissal so visibility and the
        // component event cannot diverge.
        self.finish(ui, FileDialogStatus::Cancelled);
        true
    }

    /// Resets activation-specific model and widget state before the dialog window is shown.
    fn prepare_open(&mut self, request: FileDialogRequest) -> (String, Recti) {
        assert!(!self.active, "file dialog is already open");
        assert!(self.window.events().is_alive(), "application-owned file-dialog window was destroyed");

        let FileDialogRequest { title, initial_directory, rect } = request;
        self.current_working_directory = initial_directory;
        self.path_box
            .try_update_with(self.current_working_directory.clone(), |state, path| state.set_text(path))
            .expect("file-dialog path box must remain mounted");
        self.file_name_box
            .try_update(Textbox::clear)
            .expect("file-dialog filename box must remain mounted");
        self.folder_scroll
            .try_update(|state| state.set_offset(crate::Vec2i::default()))
            .expect("file-dialog folder scroll area must remain mounted");
        self.file_scroll
            .try_update(|state| state.set_offset(crate::Vec2i::default()))
            .expect("file-dialog file scroll area must remain mounted");
        self.refresh_entries();
        self.active = true;
        (title, rect)
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

    /// Navigates to the current directory's parent after an Up-button submission.
    fn up_submitted(&mut self, _event: &ButtonSubmitted) {
        // Resolve the parent from committed controller state; filesystem refresh remains atomic with
        // the navigation update performed by `navigate_and_refresh`.
        let parent = Path::new(&self.current_working_directory)
            .parent()
            .map(|path| path.to_string_lossy().into_owned());
        if let Some(parent) = parent {
            self.navigate_and_refresh(parent);
        }
    }

    /// Navigates to the platform home directory after a Home-button submission.
    fn home_submitted(&mut self, _event: &ButtonSubmitted) {
        // Ignore an unavailable or invalid home directory and leave the current listing unchanged.
        if let Some(home) = Self::home_dir()
            && Path::new(&home).is_dir()
        {
            self.navigate_and_refresh(home);
        }
    }

    /// Resolves and navigates to a directory submitted from the path textbox.
    fn path_submitted(&mut self, event: &TextboxSubmitted) {
        // The payload is an owned semantic snapshot, so no textbox borrow is held during navigation.
        if let Some(path) = self.resolve_directory_path(&event.text) {
            self.navigate_and_refresh(path);
        }
    }

    /// Resolves the current path textbox contents after a Go-button submission.
    fn go_submitted(&mut self, _event: &ButtonSubmitted) {
        // Go has no text payload; read the retained path box only after widget traversal has ended.
        let input = self.path_box.try_read(|state| state.text().to_owned()).unwrap_or_default();
        if let Some(path) = self.resolve_directory_path(&input) {
            self.navigate_and_refresh(path);
        }
    }

    /// Navigates to the folder named by one submitted dynamic row.
    fn folder_submitted(&mut self, event: &ListItemSubmitted) {
        // Match the display label back to the controller's authoritative directory snapshot before
        // replacing rows; the dispatcher detached the complete event batch before this call.
        let directory = self
            .folders
            .iter()
            .find(|folder| Self::folder_label(&self.current_working_directory, folder) == event.label)
            .cloned();
        if let Some(directory) = directory {
            self.navigate_and_refresh(directory);
        }
    }

    /// Copies a submitted file-row label into the filename textbox.
    fn file_submitted(&mut self, event: &ListItemSubmitted) {
        // Clone the owned label only when transferring it into the retained textbox mutation.
        self.file_name_box
            .try_update_with(event.label.clone(), |state, name| state.set_text(name))
            .expect("file-dialog filename box must remain mounted");
    }

    /// Ends the active request and queues its single terminal component event.
    fn emit_completion(&mut self, completion: FileDialogStatus) {
        // Every caller checks activity before entering, keeping duplicate terminal events a debug
        // invariant violation rather than silently changing the component state twice.
        debug_assert!(self.active);
        self.active = false;
        self.completed.borrow_mut().emit(FileDialogCompleted::new(completion));
    }

    /// Hides the modal surface before publishing one accepted or cancelled result.
    fn finish(&mut self, ui: &mut Ui<'_>, completion: FileDialogStatus) {
        // Hide the retained modal before publishing completion so later subscribers observe the
        // component as idle at the same dispatch boundary.
        ui.set_window_visible(&self.window, false)
            .expect("application-owned file-dialog window must remain registered");
        self.emit_completion(completion);
    }

    /// Accepts the current selection when the Open button is submitted.
    fn accept_submitted(&mut self, ui: &mut Ui<'_>, _event: &ButtonSubmitted) {
        // An event queued before another completion may arrive after the component became idle.
        if !self.active {
            return;
        }
        if let Some(completion) = self.accepted_status() {
            self.finish(ui, completion);
        }
    }

    /// Cancels the active request when the dialog's Cancel button is submitted.
    fn cancel_submitted(&mut self, ui: &mut Ui<'_>, _event: &ButtonSubmitted) {
        // Ignore a stale queued button submission after another path completed the request.
        if self.active {
            self.finish(ui, FileDialogStatus::Cancelled);
        }
    }

    /// Handles one unified event from the retained dialog window.
    fn window_event(&mut self, ui: &mut Ui<'_>, event: &WindowEvent) {
        // Non-close lifecycle observations remain on the shared port but do not alter file-picker
        // state. A close request converges with explicit and button cancellation.
        match event {
            WindowEvent::CloseRequested if self.active => self.finish(ui, FileDialogStatus::Cancelled),
            WindowEvent::CloseRequested | WindowEvent::GeometryChanged { .. } => {}
            WindowEvent::Minimized | WindowEvent::Maximized { .. } | WindowEvent::Restored { .. } => {
                // File-dialog windows do not expose minimize/maximize buttons, so these variants
                // are exhaustive defensive handling for application-customized future options.
            }
        }
    }

    fn dispatch_up<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &ButtonSubmitted) {
        accessor(state).up_submitted(event);
    }

    fn dispatch_home<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &ButtonSubmitted) {
        accessor(state).home_submitted(event);
    }

    fn dispatch_path<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &TextboxSubmitted) {
        accessor(state).path_submitted(event);
    }

    fn dispatch_go<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &ButtonSubmitted) {
        accessor(state).go_submitted(event);
    }

    fn dispatch_folder<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &ListItemSubmitted) {
        accessor(state).folder_submitted(event);
    }

    fn dispatch_file<State>(state: &mut State, accessor: &FileDialogAccessor<State>, event: &ListItemSubmitted) {
        accessor(state).file_submitted(event);
    }

    /// Resolves the application-owned component before forwarding its Open submission.
    fn dispatch_accept<State>(state: &mut State, accessor: &FileDialogAccessor<State>, ui: &mut Ui<'_>, event: &ButtonSubmitted) {
        // The stored accessor avoids retaining a second owner or erasing application state.
        accessor(state).accept_submitted(ui, event);
    }

    /// Resolves the application-owned component before forwarding its Cancel submission.
    fn dispatch_cancel<State>(state: &mut State, accessor: &FileDialogAccessor<State>, ui: &mut Ui<'_>, event: &ButtonSubmitted) {
        // Forward the shared Ui borrow so event-time and direct cancellation use identical code.
        accessor(state).cancel_submitted(ui, event);
    }

    /// Resolves the application-owned component before forwarding one dialog-window event.
    fn dispatch_window_event<State>(state: &mut State, accessor: &FileDialogAccessor<State>, ui: &mut Ui<'_>, event: &WindowEvent) {
        // The component exhaustively interprets the concrete window event without an erased payload
        // or a second close-only event port.
        accessor(state).window_event(ui, event);
    }
}

#[allow(clippy::result_large_err)]
fn replace_column_rows(handle: &TypedWidgetHandle<Linear>, nodes: Vec<Node>) -> Result<(), Vec<Node>> {
    handle.try_update_with(nodes, |state, nodes| state.replace(nodes))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{NoopRenderer, test_atlas};
    use crate::{Button, ButtonParameters, ButtonSubmitted, Context, Dimensioni, MouseButton, SurfaceMutationError, WindowOption, rect};
    use std::{
        fs,
        panic::{AssertUnwindSafe, catch_unwind},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn dimensions() -> Dimensioni {
        Dimensioni::new(900, 700)
    }

    struct Model {
        dialog: FileDialog,
        completions: Vec<FileDialogStatus>,
        behind_submissions: usize,
    }

    impl Model {
        fn dialog_mut(state: &mut Self) -> &mut FileDialog {
            &mut state.dialog
        }

        fn completed(&mut self, event: &FileDialogCompleted) {
            self.completions.push(event.status().clone());
        }

        fn open_from_button(&mut self, ui: &mut Ui<'_>, _event: &ButtonSubmitted) {
            self.dialog.open(ui, FileDialogRequest::default());
        }

        fn behind_submitted(&mut self, _event: &ButtonSubmitted) {
            self.behind_submissions += 1;
        }
    }

    fn context_and_model() -> (Context<NoopRenderer, Model>, Model) {
        let mut context = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions());
        // The fixture window gives the component the same stable lifetime owner required from a
        // real application. Its geometry is unrelated to the dialog's screen-space rectangle.
        let owner = context.ui().create_window(Window::new(
            "file-dialog owner",
            rect(0, 0, 1, 1),
            Button::create(ButtonParameters::new("owner")).1,
        ));
        let dialog = FileDialog::new(&mut context, &owner, Model::dialog_mut);
        context.subscribe(dialog.completed(), Model::completed).unwrap();
        (
            context,
            Model {
                dialog,
                completions: Vec::new(),
                behind_submissions: 0,
            },
        )
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("microui-redux-{name}-{}-{nanos}", std::process::id()))
    }

    /// Presses one retained node through the typed window that owns its concrete widget tree.
    fn click_node(context: &mut Context<NoopRenderer, Model>, model: &mut Model, window: &WindowHandle, node: RuntimeNodeId, batched: bool) {
        // Internal geometry diagnostics resolve the private stable identity only after this test
        // helper receives the same non-owning capability used by public mutations.
        let rect = context.debug_root_node_rect(window.id(), node).expect("node rect should be laid out");
        let x = rect.x + rect.width / 2;
        let y = rect.y + rect.height / 2;
        context.mousemove(x, y);
        if !batched {
            context.update_ui_state(dimensions(), model);
        }
        context.mousedown(x, y, MouseButton::LEFT);
        context.update_ui_state(dimensions(), model);
    }

    fn release_pointer(context: &mut Context<NoopRenderer, Model>, model: &mut Model) {
        context.mouseup(0, 0, MouseButton::LEFT);
        context.update_ui_state(dimensions(), model);
    }

    #[test]
    fn request_builders_and_application_owned_open_are_public_contract() {
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

        let (mut context, mut model) = context_and_model();
        let dialog_window = model.dialog.window().clone();
        model.dialog.open(&mut context.ui(), request);
        assert!(model.dialog.is_open());
        assert_eq!(context.debug_root_name(dialog_window.id()).as_deref(), Some("Choose"));
        let window_rect = context.debug_root_rect(dialog_window.id()).unwrap();
        assert_eq!((window_rect.x, window_rect.y, window_rect.width, window_rect.height), (10, 20, 400, 300));
        assert_eq!(context.debug_modal_root(), Some(dialog_window.id()));
        assert!(model.dialog.cancel(&mut context.ui()));
        context.update_ui_state(dimensions(), &mut model);
        assert_eq!(model.completions, [FileDialogStatus::Cancelled]);
        fs::remove_dir_all(dir).unwrap();
    }

    /// Proves the component exposes the same authenticated, fallible capability as other windows.
    #[test]
    fn destroyed_dialog_window_reports_the_concrete_surface_error() {
        let (mut context, model) = context_and_model();
        let dialog_window = model.dialog.window().clone();

        // Destruction makes both stable handles stale and expires their event endpoints; subsequent
        // public mutation reports the concrete window failure instead of consulting a forgeable ID.
        context.ui().destroy_window(&dialog_window).unwrap();
        assert!(!model.dialog.window().events().is_alive());
        assert_eq!(context.ui().set_window_visible(&dialog_window, true), Err(SurfaceMutationError::UnknownWindow));
    }

    #[test]
    fn context_aware_application_handler_opens_the_component() {
        let (mut context, mut model) = context_and_model();
        let (button, node) = Button::create(ButtonParameters::new("open"));
        let button_id = node.id();
        let window = context.ui().create_window(Window::new("window", rect(0, 0, 100, 80), node));
        context.subscribe_context(button.submitted(), Model::open_from_button).unwrap();
        context.update_ui_state(dimensions(), &mut model);

        click_node(&mut context, &mut model, &window, button_id, true);
        assert!(model.dialog.is_open());
        assert_eq!(context.debug_modal_root(), Some(model.dialog.window().id()));
    }

    #[test]
    fn accepted_dialog_dispatches_completion_in_the_input_transaction() {
        let (mut context, mut model) = context_and_model();
        model
            .dialog
            .open(&mut context.ui(), FileDialogRequest::new().with_initial_directory("/retained-test"));
        context.update_ui_state(dimensions(), &mut model);
        model
            .dialog
            .file_name_box
            .try_update_with("picked.txt", |state, name| state.set_text(name))
            .unwrap();
        let dialog_window = model.dialog.window.clone();
        let open = model.dialog.test.ok_button_id;

        click_node(&mut context, &mut model, &dialog_window, open, true);
        assert_eq!(
            model.completions,
            [FileDialogStatus::Accepted(FileDialogResult {
                file_name: "picked.txt".to_owned(),
                file_path: "/retained-test/picked.txt".to_owned(),
            })]
        );
        assert!(!model.dialog.is_open());
        assert_eq!(context.debug_root_visible(dialog_window.id()), Some(false));
    }

    #[test]
    fn explicit_cancel_queues_one_completion_for_the_next_update() {
        let (mut context, mut model) = context_and_model();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        assert!(model.dialog.cancel(&mut context.ui()));
        assert!(!model.dialog.cancel(&mut context.ui()));
        assert!(model.completions.is_empty());

        context.update_ui_state(dimensions(), &mut model);
        assert_eq!(model.completions, [FileDialogStatus::Cancelled]);
    }

    #[test]
    fn sequential_opens_reuse_controls_and_reset_request_state() {
        let first_dir = unique_temp_dir("first");
        let second_dir = unique_temp_dir("second");
        fs::create_dir_all(&first_dir).unwrap();
        fs::create_dir_all(&second_dir).unwrap();
        let (mut context, mut model) = context_and_model();
        let dialog_window = model.dialog.window.clone();
        let open = model.dialog.test.ok_button_id;
        let path = model.dialog.path_box.clone();
        let filename = model.dialog.file_name_box.clone();
        let scroll = model.dialog.file_scroll.clone();

        model.dialog.open(
            &mut context.ui(),
            FileDialogRequest::new().with_title("First").with_initial_directory(first_dir.to_string_lossy()),
        );
        filename.try_update_with("stale.txt", |state, text| state.set_text(text)).unwrap();
        scroll.try_update(|state| state.set_offset(crate::vec2(0, 20))).unwrap();
        assert!(model.dialog.cancel(&mut context.ui()));
        context.update_ui_state(dimensions(), &mut model);

        model.dialog.open(
            &mut context.ui(),
            FileDialogRequest::new()
                .with_title("Second")
                .with_initial_directory(second_dir.to_string_lossy()),
        );
        assert_eq!(model.dialog.window.id(), dialog_window.id());
        assert_eq!(model.dialog.test.ok_button_id, open);
        assert_eq!(
            path.try_read(|state| state.text().to_owned()).as_deref(),
            Some(second_dir.to_string_lossy().as_ref())
        );
        assert_eq!(filename.try_read(|state| state.text().to_owned()).as_deref(), Some(""));
        let offset = scroll.try_read(|state| state.offset()).unwrap();
        assert_eq!((offset.x, offset.y), (0, 0));
        assert_eq!(context.debug_root_name(dialog_window.id()).as_deref(), Some("Second"));

        fs::remove_dir_all(first_dir).unwrap();
        fs::remove_dir_all(second_dir).unwrap();
    }

    #[test]
    fn overlapping_open_panics_without_closing_the_active_component() {
        let (mut context, mut model) = context_and_model();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        let result = catch_unwind(AssertUnwindSafe(|| {
            model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        }));
        assert!(result.is_err());
        assert!(model.dialog.is_open());
        assert_eq!(context.debug_modal_root(), Some(model.dialog.window.id()));
    }

    #[test]
    fn two_application_owned_dialogs_are_independent() {
        struct DualModel {
            first: FileDialog,
            second: FileDialog,
            first_completions: usize,
            second_completions: usize,
        }

        impl DualModel {
            fn first_mut(state: &mut Self) -> &mut FileDialog {
                &mut state.first
            }

            fn second_mut(state: &mut Self) -> &mut FileDialog {
                &mut state.second
            }

            fn first_completed(&mut self, _event: &FileDialogCompleted) {
                self.first_completions += 1;
            }

            fn second_completed(&mut self, _event: &FileDialogCompleted) {
                self.second_completions += 1;
            }
        }

        let mut context = Context::new_test_state(NoopRenderer { atlas: test_atlas() }, dimensions());
        let owner = context.ui().create_window(Window::new(
            "file-dialog owner",
            rect(0, 0, 1, 1),
            Button::create(ButtonParameters::new("owner")).1,
        ));
        let first = FileDialog::new(&mut context, &owner, DualModel::first_mut);
        let second = FileDialog::new(&mut context, &owner, DualModel::second_mut);
        context.subscribe(first.completed(), DualModel::first_completed).unwrap();
        context.subscribe(second.completed(), DualModel::second_completed).unwrap();
        let mut model = DualModel {
            first,
            second,
            first_completions: 0,
            second_completions: 0,
        };

        model.first.open(&mut context.ui(), FileDialogRequest::new().with_title("First"));
        model.second.open(&mut context.ui(), FileDialogRequest::new().with_title("Second"));
        assert_ne!(model.first.window.id(), model.second.window.id());
        assert_eq!(context.debug_modal_root(), Some(model.second.window.id()));
        assert!(model.second.cancel(&mut context.ui()));
        context.update_ui_state(dimensions(), &mut model);
        assert_eq!((model.first_completions, model.second_completions), (0, 1));
        assert!(model.first.is_open());
        assert_eq!(context.debug_modal_root(), Some(model.first.window.id()));
    }

    #[test]
    fn open_dialog_blocks_pointer_input_to_underlying_windows() {
        let (mut context, mut model) = context_and_model();
        let (button, button_node) = Button::create(ButtonParameters::new("behind"));
        context.subscribe(button.submitted(), Model::behind_submitted).unwrap();
        let window = context.ui().create_window(Window::new("window", rect(0, 0, 100, 80), button_node));
        context
            .ui()
            .set_window_options(&window, WindowOption::FRAME | WindowOption::NO_TITLE | WindowOption::NO_RESIZE)
            .unwrap();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        context.update_ui_state(dimensions(), &mut model);

        context.mousedown(10, 10, MouseButton::LEFT);
        context.mouseup(10, 10, MouseButton::LEFT);
        context.update_ui_state(dimensions(), &mut model);
        assert_eq!(model.behind_submissions, 0);
        assert!(model.dialog.is_open());
        assert_eq!(context.debug_rendered_root_names(), ["file-dialog owner", "window", "Open File"]);
    }

    #[test]
    fn action_buttons_keep_standard_height_and_browser_absorbs_resize() {
        let (mut context, mut model) = context_and_model();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        context.update_ui_state(dimensions(), &mut model);
        let dialog_window = model.dialog.window.clone();
        let toolbar_before = context.debug_root_node_rect(dialog_window.id(), model.dialog.test.up_button_id).unwrap();
        let cancel_before = context.debug_root_node_rect(dialog_window.id(), model.dialog.test.cancel_button_id).unwrap();
        let open_before = context.debug_root_node_rect(dialog_window.id(), model.dialog.test.ok_button_id).unwrap();
        assert_eq!(cancel_before.height, toolbar_before.height);
        assert_eq!(open_before.height, toolbar_before.height);

        let mut resized = context.debug_root_rect(dialog_window.id()).unwrap();
        resized.height += 80;
        context.ui().set_window_rect(&dialog_window, resized).unwrap();
        context.update_ui_state(dimensions(), &mut model);
        let toolbar_after = context.debug_root_node_rect(dialog_window.id(), model.dialog.test.up_button_id).unwrap();
        let open_after = context.debug_root_node_rect(dialog_window.id(), model.dialog.test.ok_button_id).unwrap();
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
        let (mut context, mut model) = context_and_model();
        model
            .dialog
            .open(&mut context.ui(), FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        context.update_ui_state(dimensions(), &mut model);
        let dialog_window = model.dialog.window.clone();
        let file_node = model.dialog.test.file_item_ids[0];
        click_node(&mut context, &mut model, &dialog_window, file_node, batched);
        assert_eq!(
            model.dialog.file_name_box.try_read(|state| state.text().to_owned()).as_deref(),
            Some("picked.txt")
        );
        release_pointer(&mut context, &mut model);
        let open = model.dialog.test.ok_button_id;
        click_node(&mut context, &mut model, &dialog_window, open, batched);
        assert_eq!(
            model.completions,
            [FileDialogStatus::Accepted(FileDialogResult {
                file_name: "picked.txt".to_owned(),
                file_path: file_path.to_string_lossy().into_owned(),
            })]
        );
        assert!(!model.dialog.is_open());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selecting_file_then_open_accepts_and_hides_window() {
        selection_flow(false);
    }

    #[test]
    fn clicks_work_when_move_and_press_are_queued_together() {
        selection_flow(true);
    }

    #[test]
    fn empty_accept_stays_open_and_cancel_button_completes() {
        let (mut context, mut model) = context_and_model();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        context.update_ui_state(dimensions(), &mut model);
        let dialog_window = model.dialog.window.clone();
        let open = model.dialog.test.ok_button_id;
        let cancel = model.dialog.test.cancel_button_id;
        click_node(&mut context, &mut model, &dialog_window, open, false);
        assert!(model.dialog.is_open());
        assert!(model.completions.is_empty());

        release_pointer(&mut context, &mut model);
        click_node(&mut context, &mut model, &dialog_window, cancel, false);
        assert_eq!(model.completions, [FileDialogStatus::Cancelled]);
        assert!(!model.dialog.is_open());
    }

    #[test]
    fn close_request_cancels_and_hides_the_dialog_window() {
        let (mut context, mut model) = context_and_model();
        model.dialog.open(&mut context.ui(), FileDialogRequest::default());
        context.update_ui_state(dimensions(), &mut model);
        let dialog_window = model.dialog.window.clone();
        let close = context
            .debug_root_chrome(dialog_window.id())
            .unwrap()
            .1
            .expect("dialog should have a close button");
        context.mousemove(close.x + close.width / 2, close.y + close.height / 2);
        context.mousedown(close.x + close.width / 2, close.y + close.height / 2, MouseButton::LEFT);
        context.mouseup(close.x + close.width / 2, close.y + close.height / 2, MouseButton::LEFT);
        context.update_ui_state(dimensions(), &mut model);
        assert_eq!(model.completions, [FileDialogStatus::Cancelled]);
        assert!(!model.dialog.is_open());
        assert_eq!(context.debug_root_visible(dialog_window.id()), Some(false));
    }

    #[test]
    fn folder_submission_navigates_and_replaces_only_dynamic_rows() {
        let dir = unique_temp_dir("navigate");
        let child = dir.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("inside.txt"), b"inside").unwrap();
        let (mut context, mut model) = context_and_model();
        model
            .dialog
            .open(&mut context.ui(), FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        context.update_ui_state(dimensions(), &mut model);
        let index = model
            .dialog
            .folders
            .iter()
            .position(|folder| Path::new(folder) == child)
            .expect("child directory should be listed");
        let dialog_window = model.dialog.window.clone();
        let child_node = model.dialog.test.folder_item_ids[index];
        let folder_scroll = model.dialog.folder_scroll.clone();
        let path_box = model.dialog.path_box.clone();
        click_node(&mut context, &mut model, &dialog_window, child_node, false);
        assert_eq!(Path::new(&model.dialog.current_working_directory), child);
        assert_eq!(model.dialog.files, ["inside.txt"]);
        assert!(folder_scroll.is_alive());
        assert!(path_box.is_alive());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refresh_preserves_handles_and_clamps_scroll() {
        let dir = unique_temp_dir("refresh");
        fs::create_dir_all(&dir).unwrap();
        for index in 0..60 {
            fs::write(dir.join(format!("file-{index:02}.txt")), b"row").unwrap();
        }
        let (mut context, mut model) = context_and_model();
        model
            .dialog
            .open(&mut context.ui(), FileDialogRequest::new().with_initial_directory(dir.to_string_lossy()));
        context.update_ui_state(dimensions(), &mut model);
        let path = model.dialog.path_box.clone();
        let folder_scroll = model.dialog.folder_scroll.clone();
        let file_scroll = model.dialog.file_scroll.clone();
        file_scroll.try_update(|state| state.set_offset(crate::vec2(0, 40))).unwrap();
        fs::write(dir.join("new-file.txt"), b"row").unwrap();
        model.dialog.refresh_entries();
        context.update_ui_state(dimensions(), &mut model);
        assert!(path.is_alive() && folder_scroll.is_alive() && file_scroll.is_alive());
        assert_eq!(file_scroll.try_read(|state| state.offset().y), Some(40));

        for entry in fs::read_dir(&dir).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        model.dialog.refresh_entries();
        context.update_ui_state(dimensions(), &mut model);
        assert_eq!(file_scroll.try_read(|state| state.offset().y), Some(0));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unavailable_column_replacement_returns_every_node() {
        let (column, owner) = Linear::create(LinearParameters::vertical(std::iter::empty::<Node>()));
        let rejected = column
            .try_read(|_| {
                let replacements = vec![FileDialog::static_item("one"), FileDialog::static_item("two")];
                replace_column_rows(&column, replacements).expect_err("active read must reject mutation")
            })
            .unwrap();
        assert_eq!(rejected.len(), 2);
        drop(owner);
    }
}
