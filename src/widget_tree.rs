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
// The retained widget tree owns the long-lived UI structure. Composite nodes
// such as headers, tree nodes, and embedded containers store their child lists
// here and keep stable NodeIds across frames. Each frame the container uses the
// previous-frame cache for committed geometry/results, then traverses the retained
// nodes directly through the normal layout and widget paths. Layout and interaction
// are cached in separate generations so previous-frame geometry can be consulted
// without conflating it with current-frame widget results.

mod builder;
mod cache;
mod node;
mod retained;

pub use builder::{NodeOptions, WidgetTreeBuilder};
pub use node::{NodeId, Policy, WidgetTree, WidgetTreeNode};
pub use retained::{widget_handle, WidgetHandle};

pub(crate) use cache::{NodeInteraction, NodeLayout, WidgetTreeCache};
pub(crate) use node::WidgetTreeNodeKind;
pub(crate) use retained::{erased_widget_state, widget_handle_id, TreeCustomRender, WidgetStateHandleDyn};

#[cfg(test)]
mod tests;
