//! Bookmarks: toggle / list / persist (bookmarks.json next to the renderer).

use super::*;

impl GwbDocument {
    // ---- bookmarks: toggle / list / persist ------------------------------

    pub(super) fn bookmarks_path() -> &'static str {
        "bookmarks.json"
    }

    pub(super) fn load_bookmarks(&mut self) {
        let text = match std::fs::read_to_string(Self::bookmarks_path()) {
            Ok(t) => t,
            Err(_) => return, // no file yet: fine, starts empty
        };
        let Ok(serde_json::Value::Array(items)) = serde_json::from_str(&text) else {
            crate::logger::log("ui", "bookmarks.json is malformed; ignoring");
            return;
        };
        for item in items {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let target = item.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if target.is_empty() || self.is_bookmarked(&target) {
                continue;
            }
            let (row_node, remove_node) = self.create_bookmark_row(&title, &target);
            self.bookmarks.push(Bookmark { title, target, row_node, remove_node });
        }
        self.refresh_bookmarks_empty_state();
        crate::logger::log("ui", &format!("loaded {} bookmark(s)", self.bookmarks.len()));
    }

    pub(super) fn save_bookmarks(&self) {
        let items: Vec<serde_json::Value> = self
            .bookmarks
            .iter()
            .map(|b| serde_json::json!({ "title": b.title, "target": b.target }))
            .collect();
        let text = match serde_json::to_string_pretty(&serde_json::Value::Array(items)) {
            Ok(t) => t,
            Err(e) => {
                crate::logger::log("ui", &format!("bookmarks serialize failed: {e}"));
                return;
            }
        };
        if let Err(e) = std::fs::write(Self::bookmarks_path(), text) {
            crate::logger::log("ui", &format!("bookmarks.json write failed: {e}"));
        }
    }

    pub(super) fn is_bookmarked(&self, target: &str) -> bool {
        self.bookmarks.iter().any(|b| b.target == target)
    }

    /// Toggle the bookmark for the active tab's current target (mirrors a
    /// browser's address-bar star: click again to remove).
    pub(super) fn toggle_bookmark(&mut self) {
        let Some(tab) = self.tabs.get(self.active_idx) else { return };
        if tab.target.is_empty() {
            return;
        }
        let target = tab.target.clone();
        let title = tab.title.clone();
        if let Some(pos) = self.bookmarks.iter().position(|b| b.target == target) {
            let bm = self.bookmarks.remove(pos);
            let mut m = self.doc.mutate();
            m.drop_node(bm.row_node);
            drop(m);
            crate::logger::log("ui", &format!("bookmark removed: {target}"));
        } else {
            let (row_node, remove_node) = self.create_bookmark_row(&title, &target);
            self.bookmarks.push(Bookmark { title, target: target.clone(), row_node, remove_node });
            crate::logger::log("ui", &format!("bookmark added: {target}"));
        }
        self.refresh_bookmarks_empty_state();
        self.refresh_bookmark_star();
        self.save_bookmarks();
        self.dirty = true;
    }

    pub(super) fn remove_bookmark(&mut self, target: &str) {
        let Some(pos) = self.bookmarks.iter().position(|b| b.target == target) else { return };
        let bm = self.bookmarks.remove(pos);
        let mut m = self.doc.mutate();
        m.drop_node(bm.row_node);
        drop(m);
        crate::logger::log("ui", &format!("bookmark removed: {target}"));
        self.refresh_bookmarks_empty_state();
        self.refresh_bookmark_star();
        self.save_bookmarks();
        self.dirty = true;
    }

    pub(super) fn open_bookmark(&mut self, target: String) {
        self.navigate_active(&target);
        self.set_bookmarks_panel_open(false);
    }

    /// Build one row (title, url, remove button) and insert it just before
    /// the panel's permanent "no bookmarks yet" node — same pattern as tab
    /// chips inserting before `#btn-newtab`.
    pub(super) fn create_bookmark_row(&mut self, title: &str, target: &str) -> (usize, usize) {
        let mut m = self.doc.mutate();
        let row = m.create_element(qn("div"), Vec::new());
        m.set_attr(row, attr_qn("class"), "bm-row");
        let title_el = m.create_element(qn("span"), Vec::new());
        m.set_attr(title_el, attr_qn("class"), "bm-title");
        m.set_html(title_el, &escape_html(&truncate_title(title)));
        let url_el = m.create_element(qn("span"), Vec::new());
        m.set_attr(url_el, attr_qn("class"), "bm-url");
        m.set_html(url_el, &escape_html(target));
        let remove = m.create_element(qn("button"), Vec::new());
        m.set_attr(remove, attr_qn("class"), "bm-remove");
        m.set_html(remove, "remove");
        m.append_children(row, &[title_el, url_el, remove]);
        m.insert_nodes_before(self.chrome.bookmarks_empty, &[row]);
        drop(m);
        (row, remove)
    }

    pub(super) fn refresh_bookmarks_empty_state(&mut self) {
        let show_empty = self.bookmarks.is_empty();
        let mut m = self.doc.mutate();
        m.set_style_property(
            self.chrome.bookmarks_empty,
            "display",
            if show_empty { "block" } else { "none" },
        );
        drop(m);
        self.dirty = true;
    }

    /// Sync the address-bar star to whether the active tab's target is
    /// bookmarked — called on navigate/activate so it never goes stale.
    pub(super) fn refresh_bookmark_star(&mut self) {
        let target = self.tabs.get(self.active_idx).map(|t| t.target.clone()).unwrap_or_default();
        let active = !target.is_empty() && self.is_bookmarked(&target);
        let mut m = self.doc.mutate();
        // Two layered engine quirks ruled out simpler approaches: (1) toggling
        // a CLASS here (like tab chips use) hits a real bug where a class
        // attribute already present from the static SHELL_HTML parse doesn't
        // get replaced by a runtime set_attribute of the same name — it
        // appends a second "class" attr instead, and style matching only
        // sees the first (stale) one (see attr_qn's doc comment — fixed for
        // class/value elsewhere, but simplest to just avoid class-matching
        // here). (2) The specialized set_style_property mutator marks the
        // node itself dirty but never propagates that up through ancestors
        // (unlike set_attribute, which explicitly calls mark_ancestors_dirty)
        // — Stylo's restyle traversal starts at the root and only descends
        // into subtrees an ancestor flagged dirty, so a color-only change
        // through that API is silently never repainted. Setting `style` as a
        // plain HTML attribute goes through set_attribute's code path
        // instead, which does propagate correctly.
        m.set_attr(
            self.chrome.ab_bookmark,
            attr_qn("style"),
            &format!("color: {}", if active { "#ffb400" } else { "#9fa4ab" }),
        );
        drop(m);
        self.dirty = true;
    }

    pub(super) fn set_bookmarks_panel_open(&mut self, open: bool) {
        if self.bookmarks_panel_open == open {
            return;
        }
        self.bookmarks_panel_open = open;
        let mut m = self.doc.mutate();
        m.set_style_property(
            self.chrome.bookmarks_panel,
            "display",
            if open { "flex" } else { "none" },
        );
        drop(m);
        self.dirty = true;
    }
}
