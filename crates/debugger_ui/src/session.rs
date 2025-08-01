pub mod running;

use crate::{StackTraceView, persistence::SerializedLayout, session::running::DebugTerminal};
use dap::client::SessionId;
use gpui::{
    App, Axis, Entity, EventEmitter, FocusHandle, Focusable, Subscription, Task, WeakEntity,
};
use project::Project;
use project::debugger::session::Session;
use project::worktree_store::WorktreeStore;
use rpc::proto;
use running::RunningState;
use std::{cell::OnceCell, sync::OnceLock};
use ui::{Indicator, prelude::*};
use workspace::{
    CollaboratorId, FollowableItem, ViewId, Workspace,
    item::{self, Item},
};

pub struct DebugSession {
    remote_id: Option<workspace::ViewId>,
    running_state: Entity<RunningState>,
    label: OnceLock<SharedString>,
    stack_trace_view: OnceCell<Entity<StackTraceView>>,
    _worktree_store: WeakEntity<WorktreeStore>,
    workspace: WeakEntity<Workspace>,
    _subscriptions: [Subscription; 1],
}

#[derive(Debug)]
pub enum DebugPanelItemEvent {
    Close,
    Stopped { go_to_stack_frame: bool },
}

impl DebugSession {
    pub(crate) fn running(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        parent_terminal: Option<Entity<DebugTerminal>>,
        session: Entity<Session>,
        serialized_layout: Option<SerializedLayout>,
        dock_axis: Axis,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let running_state = cx.new(|cx| {
            RunningState::new(
                session.clone(),
                project.clone(),
                workspace.clone(),
                parent_terminal,
                serialized_layout,
                dock_axis,
                window,
                cx,
            )
        });

        cx.new(|cx| Self {
            _subscriptions: [cx.subscribe(&running_state, |_, _, _, cx| {
                cx.notify();
            })],
            remote_id: None,
            running_state,
            label: OnceLock::new(),
            stack_trace_view: OnceCell::new(),
            _worktree_store: project.read(cx).worktree_store().downgrade(),
            workspace,
        })
    }

    pub(crate) fn session_id(&self, cx: &App) -> SessionId {
        self.running_state.read(cx).session_id()
    }

    pub(crate) fn stack_trace_view(
        &mut self,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> &Entity<StackTraceView> {
        let workspace = self.workspace.clone();
        let running_state = self.running_state.clone();

        self.stack_trace_view.get_or_init(|| {
            let stackframe_list = running_state.read(cx).stack_frame_list().clone();

            let stack_frame_view = cx.new(|cx| {
                StackTraceView::new(
                    workspace.clone(),
                    project.clone(),
                    stackframe_list,
                    window,
                    cx,
                )
            });

            stack_frame_view
        })
    }

    pub fn session(&self, cx: &App) -> Entity<Session> {
        self.running_state.read(cx).session().clone()
    }

    pub(crate) fn shutdown(&mut self, cx: &mut Context<Self>) {
        self.running_state
            .update(cx, |state, cx| state.shutdown(cx));
    }

    pub(crate) fn label(&self, cx: &App) -> SharedString {
        if let Some(label) = self.label.get() {
            return label.clone();
        }

        let session = self.running_state.read(cx).session();

        self.label
            .get_or_init(|| session.read(cx).label())
            .to_owned()
    }

    pub(crate) fn running_state(&self) -> &Entity<RunningState> {
        &self.running_state
    }

    pub(crate) fn label_element(&self, depth: usize, cx: &App) -> AnyElement {
        let label = self.label(cx);

        let is_terminated = self
            .running_state
            .read(cx)
            .session()
            .read(cx)
            .is_terminated();
        let icon = {
            if is_terminated {
                Some(Indicator::dot().color(Color::Error))
            } else {
                match self
                    .running_state
                    .read(cx)
                    .thread_status(cx)
                    .unwrap_or_default()
                {
                    project::debugger::session::ThreadStatus::Stopped => {
                        Some(Indicator::dot().color(Color::Conflict))
                    }
                    _ => Some(Indicator::dot().color(Color::Success)),
                }
            }
        };

        h_flex()
            .ml(depth * px(16.0))
            .gap_2()
            .when_some(icon, |this, indicator| this.child(indicator))
            .justify_between()
            .child(
                Label::new(label)
                    .size(LabelSize::Small)
                    .when(is_terminated, |this| this.strikethrough()),
            )
            .into_any_element()
    }
}

impl EventEmitter<DebugPanelItemEvent> for DebugSession {}

impl Focusable for DebugSession {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.running_state.focus_handle(cx)
    }
}

impl Item for DebugSession {
    type Event = DebugPanelItemEvent;
    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        "Debugger".into()
    }
}

impl FollowableItem for DebugSession {
    fn remote_id(&self) -> Option<workspace::ViewId> {
        self.remote_id
    }

    fn to_state_proto(&self, _window: &Window, _cx: &App) -> Option<proto::view::Variant> {
        None
    }

    fn from_state_proto(
        _workspace: Entity<Workspace>,
        _remote_id: ViewId,
        _state: &mut Option<proto::view::Variant>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<gpui::Task<anyhow::Result<Entity<Self>>>> {
        None
    }

    fn add_event_to_update_proto(
        &self,
        _event: &Self::Event,
        _update: &mut Option<proto::update_view::Variant>,
        _window: &Window,
        _cx: &App,
    ) -> bool {
        // update.get_or_insert_with(|| proto::update_view::Variant::DebugPanel(Default::default()));

        true
    }

    fn apply_update_proto(
        &mut self,
        _project: &Entity<project::Project>,
        _message: proto::update_view::Variant,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<()>> {
        Task::ready(Ok(()))
    }

    fn set_leader_id(
        &mut self,
        _leader_id: Option<CollaboratorId>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn to_follow_event(_event: &Self::Event) -> Option<workspace::item::FollowEvent> {
        None
    }

    fn dedup(&self, existing: &Self, _window: &Window, cx: &App) -> Option<workspace::item::Dedup> {
        if existing.session_id(cx) == self.session_id(cx) {
            Some(item::Dedup::KeepExisting)
        } else {
            None
        }
    }

    fn is_project_item(&self, _window: &Window, _cx: &App) -> bool {
        true
    }
}

impl Render for DebugSession {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.running_state
            .update(cx, |this, cx| this.render(window, cx).into_any_element())
    }
}
