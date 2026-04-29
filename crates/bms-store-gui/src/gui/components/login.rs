use dioxus::prelude::*;

use crate::auth;
use bms_store_storage::store::user_store::{User, UserRole, UserStore};

/// Login screen shown when users exist but nobody is logged in.
#[component]
pub fn LoginScreen(user_store: UserStore, on_login: EventHandler<User>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    let attempt_login = {
        let store = user_store.clone();
        move || {
            let store = store.clone();
            let uname = username.read().clone();
            let pw = password.read().clone();
            if uname.is_empty() || pw.is_empty() {
                error.set(Some("Username and password are required.".into()));
                return;
            }
            loading.set(true);
            error.set(None);
            spawn(async move {
                match store.authenticate(&uname, &pw).await {
                    Ok(user) => {
                        on_login.call(user);
                    }
                    Err(e) => {
                        error.set(Some(format!("{e}")));
                        loading.set(false);
                    }
                }
            });
        }
    };

    let login_click = {
        let mut f = attempt_login.clone();
        move |_: Event<MouseData>| f()
    };
    let login_key_user = {
        let mut f = attempt_login.clone();
        move |e: Event<KeyboardData>| {
            if e.key() == Key::Enter {
                f();
            }
        }
    };
    let login_key_pass = {
        let mut f = attempt_login.clone();
        move |e: Event<KeyboardData>| {
            if e.key() == Key::Enter {
                f();
            }
        }
    };

    rsx! {
        div { class: "login-backdrop",
            div { class: "login-card",
                h2 { class: "login-title", "Sign In" }
                p { class: "login-subtitle", "Enter your credentials to continue." }

                div { class: "login-field",
                    label { "Username" }
                    input {
                        r#type: "text",
                        value: "{username}",
                        placeholder: "Username",
                        autofocus: true,
                        oninput: move |e| username.set(e.value()),
                        onkeypress: login_key_user,
                    }
                }

                div { class: "login-field",
                    label { "Password" }
                    input {
                        r#type: "password",
                        value: "{password}",
                        placeholder: "Password",
                        oninput: move |e| password.set(e.value()),
                        onkeypress: login_key_pass,
                    }
                }

                if let Some(err) = error.read().as_ref() {
                    p { class: "login-error", "{err}" }
                }

                button {
                    class: "btn btn-primary login-btn",
                    disabled: *loading.read(),
                    onclick: login_click,
                    if *loading.read() { "Signing in..." } else { "Sign In" }
                }
            }
        }
    }
}

/// First-launch admin creation screen.
#[component]
pub fn AdminSetup(user_store: UserStore, on_login: EventHandler<User>) -> Element {
    let mut username = use_signal(|| "admin".to_string());
    let mut display_name = use_signal(|| "Administrator".to_string());
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    let attempt_create = {
        let store = user_store.clone();
        move || {
            let uname = username.read().clone();
            let dname = display_name.read().clone();
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
            let store = store.clone();

            spawn(async move {
                let pw_clone = pw.clone();
                let hash_result =
                    tokio::task::spawn_blocking(move || auth::hash_password(&pw_clone)).await;
                let hash = match hash_result {
                    Ok(Ok(h)) => h,
                    Ok(Err(e)) => {
                        error.set(Some(format!("Hash error: {e}")));
                        loading.set(false);
                        return;
                    }
                    Err(e) => {
                        error.set(Some(format!("Internal error: {e}")));
                        loading.set(false);
                        return;
                    }
                };

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;

                let user = User {
                    id: uuid::Uuid::new_v4().to_string(),
                    username: uname,
                    display_name: dname,
                    role: UserRole::Admin,
                    password_hash: hash,
                    created_ms: now,
                    last_login_ms: Some(now),
                    disabled: false,
                };

                match store.create_user(user.clone()).await {
                    Ok(created) => {
                        on_login.call(created);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to create admin: {e}")));
                        loading.set(false);
                    }
                }
            });
        }
    };

    let create_click = {
        let mut f = attempt_create.clone();
        move |_: Event<MouseData>| f()
    };
    let create_key = {
        let mut f = attempt_create.clone();
        move |e: Event<KeyboardData>| {
            if e.key() == Key::Enter {
                f();
            }
        }
    };

    rsx! {
        div { class: "login-backdrop",
            div { class: "login-card",
                h2 { class: "login-title", "Create Administrator Account" }
                p { class: "login-subtitle", "Set up the first admin account for this project." }

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
                    label { "Password" }
                    input {
                        r#type: "password",
                        value: "{password}",
                        placeholder: "Min 4 characters",
                        oninput: move |e| password.set(e.value()),
                    }
                }

                div { class: "login-field",
                    label { "Confirm Password" }
                    input {
                        r#type: "password",
                        value: "{confirm}",
                        placeholder: "Re-enter password",
                        oninput: move |e| confirm.set(e.value()),
                        onkeypress: create_key,
                    }
                }

                if let Some(err) = error.read().as_ref() {
                    p { class: "login-error", "{err}" }
                }

                button {
                    class: "btn btn-primary login-btn",
                    disabled: *loading.read(),
                    onclick: create_click,
                    if *loading.read() { "Creating..." } else { "Create Account" }
                }
            }
        }
    }
}
