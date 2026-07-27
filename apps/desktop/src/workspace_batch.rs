use viewer_model::{MAX_PANES, PaneId, Workspace};

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct PaneBatchChanges {
    pub(crate) added: Vec<PaneId>,
    pub(crate) removed: Vec<PaneId>,
}

pub(crate) fn resize_workspace_for_batch(
    workspace: &mut Workspace,
    requested: usize,
) -> PaneBatchChanges {
    let target_count = requested.clamp(1, MAX_PANES);
    let previous_active = workspace.active_pane;
    let mut changes = PaneBatchChanges::default();
    while workspace.panes.len() < target_count {
        let Ok(pane_id) = workspace.add_pane() else {
            break;
        };
        changes.added.push(pane_id);
    }
    while workspace.panes.len() > target_count {
        let pane_id = workspace.panes.last().expect("pane exists").id;
        if workspace.remove_pane(pane_id).is_err() {
            break;
        }
        changes.removed.push(pane_id);
    }
    workspace.active_pane = previous_active
        .filter(|active| workspace.panes.iter().any(|pane| pane.id == *active))
        .or_else(|| workspace.panes.first().map(|pane| pane.id));
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_resize_never_exceeds_the_eight_pane_limit() {
        let mut workspace = Workspace::demo();
        resize_workspace_for_batch(&mut workspace, 20);
        assert_eq!(workspace.panes.len(), MAX_PANES);
    }

    #[test]
    fn larger_batches_grow_the_grid_without_changing_the_active_pane() {
        let mut workspace = Workspace::demo();
        workspace.active_pane = Some(PaneId(2));

        assert_eq!(
            resize_workspace_for_batch(&mut workspace, 6),
            PaneBatchChanges {
                added: vec![PaneId(5), PaneId(6)],
                removed: Vec::new(),
            }
        );
        assert_eq!(workspace.panes.len(), 6);
        assert_eq!(workspace.active_pane, Some(PaneId(2)));
    }

    #[test]
    fn a_single_selected_image_reduces_the_workspace_to_one_pane() {
        let mut workspace = Workspace::demo();
        let changes = resize_workspace_for_batch(&mut workspace, 1);

        assert_eq!(workspace.panes.len(), 1);
        assert_eq!(changes.added, Vec::<PaneId>::new());
        assert_eq!(changes.removed, vec![PaneId(4), PaneId(3), PaneId(2)]);
        assert_eq!(workspace.active_pane, Some(PaneId(1)));
    }
}
