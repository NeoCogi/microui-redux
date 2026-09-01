//! The FileDialog guide uses stable window handles, context subscriptions, and Ui-bound opening.

#![allow(dead_code)]

use microui_redux::prelude::*;

struct Model {
    file_dialog: FileDialog,
    last_completion: Option<FileDialogStatus>,
}

impl Model {
    fn file_dialog_mut(state: &mut Self) -> &mut FileDialog {
        &mut state.file_dialog
    }

    fn show_file_dialog(&mut self, ui: &mut Ui<'_>, _event: &ButtonSubmitted) {
        if !self.file_dialog.is_open() {
            self.file_dialog.open(ui, FileDialogRequest::default());
        }
    }

    fn file_dialog_completed(&mut self, event: &FileDialogCompleted) {
        self.last_completion = Some(event.status().clone());
    }
}

fn install_file_dialog<B: RendererBackend>(context: &mut Context<B, Model>, owner_window: &WindowHandle) -> Model {
    let file_dialog = FileDialog::new(context, owner_window, Model::file_dialog_mut);
    context
        .subscribe(file_dialog.completed(), Model::file_dialog_completed)
        .expect("the new completion source has no previous subscriber");
    Model { file_dialog, last_completion: None }
}

fn main() {
    // Generic function bodies validate the public setup without requiring a platform backend.
}
