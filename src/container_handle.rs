//! Shared container handles used by windows, panels, and retained container nodes.

use std::{
    cell::{Ref, RefCell, RefMut},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use crate::canvas::Canvas;
use crate::container::Container;
use crate::render::Renderer;

pub(crate) type ContainerId = *const ();

pub(crate) fn container_id_of(handle: &ContainerHandle) -> ContainerId {
    Rc::as_ptr(&handle.0) as *const ()
}

#[derive(Clone)]
/// Shared handle to a container that can be embedded inside windows or panels.
pub struct ContainerHandle(pub(crate) Rc<RefCell<Container>>);

/// Read-only view into a container borrowed from a handle.
pub struct ContainerView<'a> {
    inner: &'a Container,
}

impl<'a> ContainerView<'a> {
    fn new(inner: &'a Container) -> Self {
        Self { inner }
    }
}

impl<'a> Deref for ContainerView<'a> {
    type Target = Container;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

/// Mutable view into a container borrowed from a handle.
pub struct ContainerViewMut<'a> {
    inner: &'a mut Container,
}

impl<'a> ContainerViewMut<'a> {
    fn new(inner: &'a mut Container) -> Self {
        Self { inner }
    }
}

impl<'a> Deref for ContainerViewMut<'a> {
    type Target = Container;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a> DerefMut for ContainerViewMut<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl ContainerHandle {
    pub(crate) fn new(container: Container) -> Self {
        Self(Rc::new(RefCell::new(container)))
    }

    pub(crate) fn render<R: Renderer>(&mut self, canvas: &mut Canvas<R>) {
        self.0.borrow_mut().render(canvas)
    }

    pub(crate) fn finish(&mut self) {
        self.0.borrow_mut().finish()
    }

    /// Returns an immutable borrow of the underlying container.
    pub(crate) fn inner<'a>(&'a self) -> Ref<'a, Container> {
        self.0.borrow()
    }

    /// Returns a mutable borrow of the underlying container.
    pub(crate) fn inner_mut<'a>(&'a mut self) -> RefMut<'a, Container> {
        self.0.borrow_mut()
    }

    /// Executes `f` with a read-only view into the container.
    pub fn with<R>(&self, f: impl FnOnce(&ContainerView<'_>) -> R) -> R {
        let container = self.0.borrow();
        let view = ContainerView::new(&container);
        f(&view)
    }

    /// Executes `f` with a mutable view into the container.
    pub fn with_mut<R>(&mut self, f: impl FnOnce(&mut ContainerViewMut<'_>) -> R) -> R {
        let mut container = self.0.borrow_mut();
        let mut view = ContainerViewMut::new(&mut container);
        f(&mut view)
    }
}
