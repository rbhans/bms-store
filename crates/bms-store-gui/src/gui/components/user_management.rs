use dioxus::prelude::*;

use crate::auth::{self, AllRolePermissions, Permission, RolePermissions};
use bms_store_storage::store::user_store::{User, UserRole, UserStore};

#[derive(Debug, Clone, Copy, PartialEq)]
enum UsersSubTab {
    Users,
    RolePermissions,
}

#[component]
pub fn UserManagementView() -> Element {
    let mut sub_tab = use_signal(|| UsersSubTab::Users);
    let current = *sub_tab.read();

    rsx! {
        div { class: "users-section",
            div { class: "users-sub-tabs",
                button {
                    class: if current == UsersSubTab::Users { "schedule-tab active" } else { "schedule-tab" },
                    onclick: move |_| sub_tab.set(UsersSubTab::Users),
                    "Users"
                }
                button {
                    class: if current == UsersSubTab::RolePermissions { "schedule-tab active" } else { "schedule-tab" },
                    onclick: move |_| sub_tab.set(UsersSubTab::RolePermissions),
                    "Role Permissions"
                }
            }
            div { class: "users-sub-content",
                match current {
                    UsersSubTab::Users => rsx! { UserListView {} },
                    UsersSubTab::RolePermissions => rsx! { RolePermissionsView {} },
                }
            }
        }
    }
}

#[component]
fn UserListView() -> Element {
    let state = use_context::<crate::gui::state::AppState>();
    let user_store = state.user_store.clone();
    let current_user_id = state
        .current_user
        .read()
        .as_ref()
        .map(|u| u.id.clone())
        .unwrap_or_default();

    let mut users = use_signal(Vec::<User>::new);
    let mut selected_id = use_signal(|| Option::<String>::None);
    let mut show_add = use_signal(|| false);

    // Load users
    let load_store = user_store.clone();
    let mut users_version = use_signal(|| 0u32);
    let _load = {
        let v = *users_version.read();
        use_resource(move || {
            let store = load_store.clone();
            let _v = v;
            async move {
                let list = store.list_users().await;
                users.set(list);
            }
        })
    };

    let user_list = users.read().clone();
    let sel_id = selected_id.read().clone();
    let selected_user = sel_id
        .as_ref()
        .and_then(|id| user_list.iter().find(|u| u.id == *id).cloned());

    rsx! {
        div { class: "user-mgmt",
            // Left: user list
            div { class: "user-mgmt-list",
                div { class: "user-mgmt-list-header",
                    h3 { "Users" }
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| show_add.set(true),
                        "+ Add User"
                    }
                }

                for user in &user_list {
                    {
                        let uid = user.id.clone();
                        let is_selected = sel_id.as_deref() == Some(&user.id);
                        let is_current = user.id == current_user_id;
                        let item_class = if is_selected { "user-list-item selected" } else { "user-list-item" };
                        let role_label = user.role.label();
                        let role_class = format!("user-role-badge role-{}", role_label.to_lowercase());
                        let display = user.display_name.clone();
                        let uname = user.username.clone();
                        let is_disabled = user.disabled;
                        rsx! {
                            div {
                                class: "{item_class}",
                                onclick: move |_| {
                                    selected_id.set(Some(uid.clone()));
                                    show_add.set(false);
                                },
                                div { class: "user-list-item-info",
                                    span { class: "user-list-name",
                                        "{display}"
                                        if is_current {
                                            span { class: "user-badge-you", " (you)" }
                                        }
                                    }
                                    span { class: "user-list-username", "@{uname}" }
                                }
                                span { class: "{role_class}", "{role_label}" }
                                if is_disabled {
                                    span { class: "user-badge-disabled", "Disabled" }
                                }
                            }
                        }
                    }
                }
            }

            // Right: detail / add form
            div { class: "user-mgmt-detail",
                if *show_add.read() {
                    AddUserForm {
                        user_store: user_store.clone(),
                        on_created: move |_| {
                            show_add.set(false);
                            { let v = *users_version.read(); users_version.set(v + 1); }
                        },
                        on_cancel: move |_| show_add.set(false),
                    }
                } else if let Some(user) = selected_user {
                    UserDetailForm {
                        user: user.clone(),
                        user_store: user_store.clone(),
                        is_self: user.id == current_user_id,
                        on_updated: move |_| {
                            { let v = *users_version.read(); users_version.set(v + 1); }
                        },
                        on_deleted: move |_| {
                            selected_id.set(None);
                            { let v = *users_version.read(); users_version.set(v + 1); }
                        },
                    }
                } else {
                    div { class: "user-mgmt-placeholder",
                        p { "Select a user to view details, or add a new user." }
                    }
                }
            }
        }
    }
}

#[component]
fn AddUserForm(
    user_store: UserStore,
    on_created: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut username = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut role = use_signal(|| UserRole::Operator);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);
    let state = use_context::<crate::gui::state::AppState>();
    let create_audit = state.clone();

    let do_create = move |_| {
        let uname = username.read().clone();
        let dname = display_name.read().clone();
        let r = role.read().clone();
        let pw = password.read().clone();
        let cf = confirm.read().clone();

        if uname.is_empty() || dname.is_empty() || pw.is_empty() {
            error.set(Some("All fields are required.".into()));
            return;
        }
        if pw.len() < 4 {
            error.set(Some("Password must be at least 4 characters.".into()));
            return;
        }
        if pw != cf {
            error.set(Some("Passwords do not match.".into()));
            return;
        }

        loading.set(true);
        error.set(None);
        let store = user_store.clone();
        let audit = create_audit.clone();

        spawn(async move {
            let pw_clone = pw.clone();
            let hash =
                match tokio::task::spawn_blocking(move || auth::hash_password(&pw_clone)).await {
                    Ok(Ok(h)) => h,
                    Ok(Err(e)) => {
                        error.set(Some(format!("{e}")));
                        loading.set(false);
                        return;
                    }
                    Err(e) => {
                        error.set(Some(format!("{e}")));
                        loading.set(false);
                        return;
                    }
                };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;

            let uname_audit = uname.clone();
            let user = User {
                id: uuid::Uuid::new_v4().to_string(),
                username: uname,
                display_name: dname,
                role: r,
                password_hash: hash,
                created_ms: now,
                last_login_ms: None,
                disabled: false,
            };

            match store.create_user(user).await {
                Ok(_) => {
                    audit.audit(
                        bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                            bms_store_storage::store::audit_store::AuditAction::CreateUser,
                            "user",
                        )
                        .details(&format!("username={}", uname_audit)),
                    );
                    loading.set(false);
                    on_created.call(());
                }
                Err(e) => {
                    error.set(Some(format!("{e}")));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "user-detail-form",
            h3 { "Add User" }

            div { class: "login-field",
                label { "Username" }
                input {
                    r#type: "text",
                    value: "{username}",
                    oninput: move |e| username.set(e.value()),
                }
            }
            div { class: "login-field",
                label { "Display Name" }
                input {
                    r#type: "text",
                    value: "{display_name}",
                    oninput: move |e| display_name.set(e.value()),
                }
            }
            div { class: "login-field",
                label { "Role" }
                select {
                    value: "{role.read().label()}",
                    onchange: move |e| {
                        let val = e.value();
                        role.set(match val.as_str() {
                            "Admin" => UserRole::Admin,
                            "Operator" => UserRole::Operator,
                            _ => UserRole::Viewer,
                        });
                    },
                    for r in UserRole::all() {
                        option {
                            value: "{r.label()}",
                            selected: *role.read() == *r,
                            "{r.label()}"
                        }
                    }
                }
            }
            div { class: "login-field",
                label { "Password" }
                input {
                    r#type: "password",
                    value: "{password}",
                    oninput: move |e| password.set(e.value()),
                }
            }
            div { class: "login-field",
                label { "Confirm Password" }
                input {
                    r#type: "password",
                    value: "{confirm}",
                    oninput: move |e| confirm.set(e.value()),
                }
            }

            if let Some(err) = error.read().as_ref() {
                p { class: "login-error", "{err}" }
            }

            div { class: "user-detail-actions",
                button {
                    class: "btn btn-primary",
                    disabled: *loading.read(),
                    onclick: do_create,
                    "Create User"
                }
                button {
                    class: "btn",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
        }
    }
}

#[component]
fn UserDetailForm(
    user: User,
    user_store: UserStore,
    is_self: bool,
    on_updated: EventHandler<()>,
    on_deleted: EventHandler<()>,
) -> Element {
    let mut display_name = use_signal(|| user.display_name.clone());
    let mut role = use_signal(|| user.role.clone());
    let mut disabled = use_signal(|| user.disabled);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut success = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);
    let state = use_context::<crate::gui::state::AppState>();
    let save_audit = state.clone();
    let pw_audit = state.clone();
    let del_audit = state.clone();

    // Reset form when user changes
    let user_id = user.id.clone();
    use_effect(move || {
        display_name.set(user.display_name.clone());
        role.set(user.role.clone());
        disabled.set(user.disabled);
        new_password.set(String::new());
        confirm_password.set(String::new());
        error.set(None);
        success.set(None);
        confirm_delete.set(false);
    });

    let save_user = {
        let store = user_store.clone();
        let uid = user_id.clone();
        move |_| {
            let store = store.clone();
            let uid = uid.clone();
            let audit = save_audit.clone();
            let dn = display_name.read().clone();
            let r = role.read().clone();
            let dis = *disabled.read();
            loading.set(true);
            error.set(None);
            success.set(None);

            spawn(async move {
                let details = format!("display_name={dn} role={r:?} disabled={dis}");
                match store.update_user(&uid, &dn, r, dis).await {
                    Ok(()) => {
                        audit.audit(
                            bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                bms_store_storage::store::audit_store::AuditAction::UpdateUser,
                                "user",
                            )
                            .resource_id(&uid)
                            .details(&details),
                        );
                        success.set(Some("User updated.".into()));
                        on_updated.call(());
                    }
                    Err(e) => error.set(Some(format!("{e}"))),
                }
                loading.set(false);
            });
        }
    };

    let reset_pw = {
        let store = user_store.clone();
        let uid = user_id.clone();
        move |_| {
            let pw = new_password.read().clone();
            let cf = confirm_password.read().clone();
            if pw.is_empty() {
                error.set(Some("Enter a new password.".into()));
                return;
            }
            if pw.len() < 4 {
                error.set(Some("Password must be at least 4 characters.".into()));
                return;
            }
            if pw != cf {
                error.set(Some("Passwords do not match.".into()));
                return;
            }
            let store = store.clone();
            let uid = uid.clone();
            let audit = pw_audit.clone();
            loading.set(true);
            error.set(None);
            success.set(None);

            spawn(async move {
                let pw_clone = pw.clone();
                let hash = match tokio::task::spawn_blocking(move || auth::hash_password(&pw_clone))
                    .await
                {
                    Ok(Ok(h)) => h,
                    Ok(Err(e)) => {
                        error.set(Some(format!("{e}")));
                        loading.set(false);
                        return;
                    }
                    Err(e) => {
                        error.set(Some(format!("{e}")));
                        loading.set(false);
                        return;
                    }
                };
                match store.update_password(&uid, &hash).await {
                    Ok(()) => {
                        audit.audit(
                            bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                bms_store_storage::store::audit_store::AuditAction::ChangePassword,
                                "user",
                            )
                            .resource_id(&uid),
                        );
                        success.set(Some("Password updated.".into()));
                        new_password.set(String::new());
                        confirm_password.set(String::new());
                    }
                    Err(e) => error.set(Some(format!("{e}"))),
                }
                loading.set(false);
            });
        }
    };

    let delete_user = {
        let store = user_store.clone();
        let uid = user_id.clone();
        move |_| {
            if !*confirm_delete.read() {
                confirm_delete.set(true);
                return;
            }
            let store = store.clone();
            let uid = uid.clone();
            let audit = del_audit.clone();
            loading.set(true);
            spawn(async move {
                match store.delete_user(&uid).await {
                    Ok(()) => {
                        audit.audit(
                            bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                                bms_store_storage::store::audit_store::AuditAction::DeleteUser,
                                "user",
                            )
                            .resource_id(&uid),
                        );
                        on_deleted.call(());
                    }
                    Err(e) => error.set(Some(format!("{e}"))),
                }
                loading.set(false);
            });
        }
    };

    let last_login = user.last_login_ms.map(|ms| {
        let secs = ms / 1000;
        let mins = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - secs)
            / 60;
        if mins < 1 {
            "Just now".to_string()
        } else if mins < 60 {
            format!("{mins} min ago")
        } else if mins < 1440 {
            format!("{} hr ago", mins / 60)
        } else {
            format!("{} days ago", mins / 1440)
        }
    });

    rsx! {
        div { class: "user-detail-form",
            h3 { "Edit User" }

            div { class: "user-detail-meta",
                span { "Username: @{user_id}" }
                if let Some(ref ll) = last_login {
                    span { "Last login: {ll}" }
                }
            }

            div { class: "login-field",
                label { "Display Name" }
                input {
                    r#type: "text",
                    value: "{display_name}",
                    oninput: move |e| display_name.set(e.value()),
                }
            }
            div { class: "login-field",
                label { "Role" }
                select {
                    value: "{role.read().label()}",
                    disabled: is_self,
                    onchange: move |e| {
                        let val = e.value();
                        role.set(match val.as_str() {
                            "Admin" => UserRole::Admin,
                            "Operator" => UserRole::Operator,
                            _ => UserRole::Viewer,
                        });
                    },
                    for r in UserRole::all() {
                        option {
                            value: "{r.label()}",
                            selected: *role.read() == *r,
                            "{r.label()}"
                        }
                    }
                }
                if is_self {
                    p { class: "login-hint", "Cannot change your own role." }
                }
            }
            div { class: "login-field",
                label {
                    input {
                        r#type: "checkbox",
                        checked: *disabled.read(),
                        disabled: is_self,
                        onchange: move |e| disabled.set(e.checked()),
                    }
                    " Disabled"
                }
                if is_self {
                    p { class: "login-hint", "Cannot disable your own account." }
                }
            }

            div { class: "user-detail-actions",
                button {
                    class: "btn btn-primary",
                    disabled: *loading.read(),
                    onclick: save_user,
                    "Save Changes"
                }
            }

            // Password reset section
            h4 { "Reset Password" }
            div { class: "login-field",
                label { "New Password" }
                input {
                    r#type: "password",
                    value: "{new_password}",
                    oninput: move |e| new_password.set(e.value()),
                }
            }
            div { class: "login-field",
                label { "Confirm Password" }
                input {
                    r#type: "password",
                    value: "{confirm_password}",
                    oninput: move |e| confirm_password.set(e.value()),
                }
            }
            div { class: "user-detail-actions",
                button {
                    class: "btn",
                    disabled: *loading.read(),
                    onclick: reset_pw,
                    "Reset Password"
                }
            }

            if let Some(err) = error.read().as_ref() {
                p { class: "login-error", "{err}" }
            }
            if let Some(msg) = success.read().as_ref() {
                p { class: "login-success", "{msg}" }
            }

            // Delete section
            if !is_self {
                div { class: "user-detail-danger",
                    button {
                        class: if *confirm_delete.read() { "btn btn-danger" } else { "btn" },
                        disabled: *loading.read(),
                        onclick: delete_user,
                        if *confirm_delete.read() { "Confirm Delete" } else { "Delete User" }
                    }
                    if *confirm_delete.read() {
                        button {
                            class: "btn",
                            onclick: move |_| confirm_delete.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

// ----------------------------------------------------------------
// Role Permissions Editor
// ----------------------------------------------------------------

#[component]
fn RolePermissionsView() -> Element {
    let state = use_context::<crate::gui::state::AppState>();
    let user_store = state.user_store.clone();
    let user_store2 = user_store.clone();
    let mut role_perms = state.role_permissions;

    let all_perms = role_perms.read().clone();
    let mut saving = use_signal(|| false);
    let mut status_msg = use_signal(|| Option::<String>::None);

    let toggle_permission = {
        let user_store = user_store.clone();
        let state2 = state.clone();
        move |role: UserRole, perm: Permission, current: bool| {
            let store = user_store.clone();
            let new_val = !current;
            // Optimistic update
            {
                let mut rp = role_perms.write();
                rp.for_role_mut(&role).set(perm, new_val);
            }
            state2.audit(
                bms_store_storage::store::audit_store::AuditEntryBuilder::new(
                    bms_store_storage::store::audit_store::AuditAction::ChangeRolePermission,
                    "role_permission",
                )
                .details(&format!("{:?}.{} = {new_val}", role, perm.key())),
            );
            // Persist
            spawn(async move {
                if let Err(e) = store.set_role_permission(&role, perm.key(), new_val).await {
                    status_msg.set(Some(format!("Error: {e}")));
                }
            });
        }
    };

    let reset_defaults = move |_| {
        let store = user_store2.clone();
        saving.set(true);
        status_msg.set(None);
        spawn(async move {
            let defaults = AllRolePermissions::default();
            for role in UserRole::all() {
                let role_defaults = RolePermissions::defaults(role);
                for perm in Permission::all() {
                    let val = role_defaults.get(*perm);
                    if let Err(e) = store.set_role_permission(role, perm.key(), val).await {
                        status_msg.set(Some(format!("Error: {e}")));
                        saving.set(false);
                        return;
                    }
                }
            }
            role_perms.set(defaults);
            status_msg.set(Some("Reset to defaults.".into()));
            saving.set(false);
        });
    };

    rsx! {
        div { class: "role-perms-view",
            div { class: "role-perms-header",
                h3 { "Role Permissions" }
                p { class: "role-perms-desc",
                    "Configure what each role is allowed to do. Changes take effect immediately."
                }
            }

            table { class: "role-perms-table",
                thead {
                    tr {
                        th { class: "role-perms-perm-col", "Permission" }
                        {UserRole::all().iter().map(|role| {
                            let cls = format!("user-role-badge role-{}", role.label().to_lowercase());
                            let lbl = role.label().to_string();
                            rsx! {
                                th { class: "role-perms-role-col",
                                    span { class: "{cls}", "{lbl}" }
                                }
                            }
                        })}
                    }
                }
                tbody {
                    {Permission::all().iter().map(|perm| {
                        let p = *perm;
                        let label = perm.label().to_string();
                        let desc = perm.description().to_string();
                        let key = perm.key().to_string();
                        let toggle = toggle_permission.clone();
                        rsx! {
                            tr { key: "{key}",
                                td { class: "role-perms-perm-cell",
                                    span { class: "role-perms-perm-name", "{label}" }
                                    span { class: "role-perms-perm-desc", "{desc}" }
                                }
                                {UserRole::all().iter().map(|role| {
                                    let r = role.clone();
                                    let current = all_perms.for_role(role).get(p);
                                    let mut toggle = toggle.clone();
                                    rsx! {
                                        td { class: "role-perms-check-cell",
                                            input {
                                                r#type: "checkbox",
                                                checked: current,
                                                onchange: move |_| toggle(r.clone(), p, current),
                                            }
                                        }
                                    }
                                })}
                            }
                        }
                    })}
                }
            }

            div { class: "role-perms-actions",
                button {
                    class: "btn",
                    disabled: *saving.read(),
                    onclick: reset_defaults,
                    "Reset to Defaults"
                }
                if let Some(ref msg) = *status_msg.read() {
                    span { class: "login-success", "{msg}" }
                }
            }
        }
    }
}
