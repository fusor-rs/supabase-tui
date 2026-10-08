use super::{
    Console,
    listing::{Section, Source},
};
use crate::supabase::Access;
use std::rc::Rc;

impl Console {
    pub(super) fn run_sql(self: &Rc<Self>) {
        let sql = self.sql.with_untracked(|sql| sql.trim().to_owned());
        if sql.is_empty() {
            self.inform("Write a query first.");
            return;
        }
        let Some((api, project)) = self.target() else {
            return;
        };
        let access = if self.read_only.get_untracked() {
            Access::ReadOnly
        } else {
            Access::ReadWrite
        };
        self.history.borrow_mut().clear();
        self.load(Source::Section(Section::Sql), async move {
            api.query(&project.reference, &sql, &[], access).await
        });
    }

    pub(super) fn in_sql_editor(&self) -> bool {
        self.is_section(Section::Sql)
    }

    pub(super) fn access_hint(&self) -> &'static str {
        if self.read_only.get() {
            "Runs as supabase_read_only_user"
        } else {
            "Runs as postgres · writes are permanent"
        }
    }
}
