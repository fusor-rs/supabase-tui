use super::{
    Console, Screen,
    listing::{Flag, Section, View},
    overlay::Overlay,
    projects::project_entry,
    users::UserAction,
};
use crate::supabase::{Condition, TOKEN_VARIABLE, TOKENS_PAGE};
use std::rc::Rc;

const LOGO: &str = include_str!("../../assets/logo.txt");

macro_rules! console_views {
    ($($name:ident => $template:literal),+ $(,)?) => {
        $(
            #[derive(fusor::FromInputs)]
            struct $name {
                #[input]
                console: Rc<Console>,
            }

            fusor::template!(backend = "hypercmd", $template);
        )+
    };
}

console_views! {
    Welcome => "ui/welcome.html",
    Sidebar => "ui/sidebar.html",
    GridView => "ui/grid.html",
    SqlPanel => "ui/sql.html",
    StoragePanel => "ui/storage.html",
    UsersPanel => "ui/users.html",
    AdvisorsPanel => "ui/advisors.html",
    Details => "ui/details.html",
    StatusBar => "ui/status-bar.html",
    Help => "ui/help.html",
    Record => "ui/record.html",
    Confirm => "ui/confirm.html",
    Invite => "ui/invite.html",
}

fusor::template!(backend = "hypercmd", "ui/app.html");
