use fleet_core::{ClusterId, Command};
use gpui::{AppContext, Context, Window};

use super::Shell;
use crate::kit::{ActionMenu, ConfirmDialog, MenuItem, confirm};

const REMOVE: &str = "Remove from Fleet";
const REMOVE_DESCRIPTION: &str =
    "Disconnect and forget this cluster. Your kubeconfig is not changed.";
const REMOVE_CONFIRM: &str = "Remove";

impl Shell {
    pub(super) fn sync_cluster_menus(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ClusterId> = self
            .state
            .snapshot
            .clusters
            .iter()
            .map(|cluster| cluster.id.clone())
            .collect();
        self.cluster_menus.retain(|id, _| ids.contains(id));
        let shell = cx.entity().downgrade();
        for id in ids {
            if self.cluster_menus.contains_key(&id) {
                continue;
            }
            let removed = id.clone();
            let shell = shell.clone();
            let item = MenuItem::new(REMOVE, move |window, cx| {
                let removed = removed.clone();
                shell
                    .update(cx, |_, cx| Self::confirm_remove(removed, window, cx))
                    .ok();
            })
            .destructive()
            .description(REMOVE_DESCRIPTION);
            let menu_id = format!("cluster-menu-{id}");
            let menu = cx.new(|cx| ActionMenu::new(menu_id, cx).plain().items(vec![item]));
            self.cluster_menus.insert(id, menu);
        }
    }

    fn confirm_remove(id: ClusterId, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let title = format!("Remove {id}");
        let body = format!(
            "Fleet disconnects from {id} and stops watching it. Your kubeconfig file is not \
             changed, so you can add the context again from Add cluster."
        );
        let removed = id;
        let dialog = ConfirmDialog::new(title, body, REMOVE_CONFIRM, move |_, cx| {
            let command = Command::RemoveCluster(removed.clone());
            shell
                .update(cx, |shell, cx| shell.dispatch(command, cx))
                .ok();
        });
        confirm(dialog, window, cx);
    }
}
