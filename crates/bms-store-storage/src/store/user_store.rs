use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::auth::{self, AllRolePermissions, AuthError, Permission};
pub use bms_core::rbac::UserRole;

// ----------------------------------------------------------------
// Public types
// ----------------------------------------------------------------

pub type UserId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: String,
    pub role: UserRole,
    pub password_hash: String,
    pub created_ms: i64,
    pub last_login_ms: Option<i64>,
    pub disabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("database error: {0}")]
    Db(String),
    #[error("channel closed")]
    ChannelClosed,
    #[error("not found")]
    NotFound,
    #[error("username already exists")]
    UsernameExists,
}

// ----------------------------------------------------------------
// Commands sent to the SQLite thread
// ----------------------------------------------------------------

enum UserCmd {
    CreateUser {
        user: User,
        reply: oneshot::Sender<Result<User, UserError>>,
    },
    UpdateUser {
        id: UserId,
        display_name: String,
        role: UserRole,
        disabled: bool,
        reply: oneshot::Sender<Result<(), UserError>>,
    },
    UpdatePassword {
        id: UserId,
        password_hash: String,
        reply: oneshot::Sender<Result<(), UserError>>,
    },
    DeleteUser {
        id: UserId,
        reply: oneshot::Sender<Result<(), UserError>>,
    },
    GetUser {
        id: UserId,
        reply: oneshot::Sender<Result<User, UserError>>,
    },
    GetUserByUsername {
        username: String,
        reply: oneshot::Sender<Result<User, UserError>>,
    },
    ListUsers {
        reply: oneshot::Sender<Vec<User>>,
    },
    UpdateLastLogin {
        id: UserId,
        timestamp_ms: i64,
        reply: oneshot::Sender<Result<(), UserError>>,
    },
    HasAnyUsers {
        reply: oneshot::Sender<bool>,
    },
    GetAllRolePermissions {
        reply: oneshot::Sender<AllRolePermissions>,
    },
    SetRolePermission {
        role: UserRole,
        permission_key: String,
        value: bool,
        reply: oneshot::Sender<Result<(), UserError>>,
    },
}

// ----------------------------------------------------------------
// UserStore — async handle to the SQLite thread
// ----------------------------------------------------------------

#[derive(Clone)]
pub struct UserStore {
    cmd_tx: mpsc::UnboundedSender<UserCmd>,
}

impl PartialEq for UserStore {
    fn eq(&self, _other: &Self) -> bool {
        // Dioxus needs PartialEq for component props; stores are singletons
        true
    }
}

impl UserStore {
    pub async fn create_user(&self, user: User) -> Result<User, UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::CreateUser {
                user,
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn update_user(
        &self,
        id: &str,
        display_name: &str,
        role: UserRole,
        disabled: bool,
    ) -> Result<(), UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::UpdateUser {
                id: id.to_string(),
                display_name: display_name.to_string(),
                role,
                disabled,
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn update_password(&self, id: &str, password_hash: &str) -> Result<(), UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::UpdatePassword {
                id: id.to_string(),
                password_hash: password_hash.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn delete_user(&self, id: &str) -> Result<(), UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::DeleteUser {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn get_user(&self, id: &str) -> Result<User, UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::GetUser {
                id: id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<User, UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::GetUserByUsername {
                username: username.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn list_users(&self) -> Vec<User> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UserCmd::ListUsers { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn update_last_login(&self, id: &str) -> Result<(), UserError> {
        let now = now_ms();
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::UpdateLastLogin {
                id: id.to_string(),
                timestamp_ms: now,
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    pub async fn has_any_users(&self) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UserCmd::HasAnyUsers { reply: reply_tx });
        reply_rx.await.unwrap_or(false)
    }

    pub async fn get_all_role_permissions(&self) -> AllRolePermissions {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UserCmd::GetAllRolePermissions { reply: reply_tx });
        reply_rx.await.unwrap_or_default()
    }

    pub async fn set_role_permission(
        &self,
        role: &UserRole,
        permission_key: &str,
        value: bool,
    ) -> Result<(), UserError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(UserCmd::SetRolePermission {
                role: role.clone(),
                permission_key: permission_key.to_string(),
                value,
                reply: reply_tx,
            })
            .map_err(|_| UserError::ChannelClosed)?;
        reply_rx.await.map_err(|_| UserError::ChannelClosed)?
    }

    /// Authenticate a user by username and password.
    /// Verifies the password hash off the SQLite thread using spawn_blocking.
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<User, AuthError> {
        let user = self
            .get_user_by_username(username)
            .await
            .map_err(|e| match e {
                UserError::NotFound => AuthError::InvalidCredentials,
                other => AuthError::StoreError(other.to_string()),
            })?;

        if user.disabled {
            return Err(AuthError::UserDisabled);
        }

        let hash = user.password_hash.clone();
        let pw = password.to_string();
        let valid = tokio::task::spawn_blocking(move || auth::verify_password(&pw, &hash))
            .await
            .map_err(|e| AuthError::StoreError(e.to_string()))??;

        if !valid {
            return Err(AuthError::InvalidCredentials);
        }

        // Update last login timestamp
        let _ = self.update_last_login(&user.id).await;

        Ok(user)
    }
}

// ----------------------------------------------------------------
// Schema
// ----------------------------------------------------------------

use super::migration::{run_migrations, Migration};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    label: "initial user schema",
    sql: "
CREATE TABLE IF NOT EXISTS user (
    id            TEXT PRIMARY KEY,
    username      TEXT UNIQUE NOT NULL,
    display_name  TEXT NOT NULL,
    role          TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_ms    INTEGER NOT NULL,
    last_login_ms INTEGER,
    disabled      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS role_permission (
    role           TEXT NOT NULL,
    permission_key TEXT NOT NULL,
    allowed        INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (role, permission_key)
);
",
}];

// ----------------------------------------------------------------
// Start function
// ----------------------------------------------------------------

pub fn start_user_store_with_path(db_path: &Path) -> UserStore {
    let db_dir = db_path.parent().unwrap_or(Path::new("."));
    if !db_dir.exists() {
        std::fs::create_dir_all(db_dir).expect("failed to create data directory");
    }

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

    let path_clone = db_path.to_path_buf();
    std::thread::Builder::new()
        .name("user-sqlite".into())
        .spawn(move || run_sqlite_thread(&path_clone, cmd_rx))
        .expect("failed to spawn user SQLite thread");

    UserStore { cmd_tx }
}

// ----------------------------------------------------------------
// SQLite thread
// ----------------------------------------------------------------

fn run_sqlite_thread(db_path: &Path, mut cmd_rx: mpsc::UnboundedReceiver<UserCmd>) {
    let conn = rusqlite::Connection::open(db_path).expect("failed to open users database");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
    )
    .expect("failed to set pragmas");
    run_migrations(&conn, "users", MIGRATIONS).expect("users: schema migration failed");

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            UserCmd::CreateUser { user, reply } => {
                let result = create_user_db(&conn, &user);
                let _ = reply.send(result);
            }
            UserCmd::UpdateUser {
                id,
                display_name,
                role,
                disabled,
                reply,
            } => {
                let result = update_user_db(&conn, &id, &display_name, &role, disabled);
                let _ = reply.send(result);
            }
            UserCmd::UpdatePassword {
                id,
                password_hash,
                reply,
            } => {
                let result = update_password_db(&conn, &id, &password_hash);
                let _ = reply.send(result);
            }
            UserCmd::DeleteUser { id, reply } => {
                let result = delete_user_db(&conn, &id);
                let _ = reply.send(result);
            }
            UserCmd::GetUser { id, reply } => {
                let result = get_user_db(&conn, &id);
                let _ = reply.send(result);
            }
            UserCmd::GetUserByUsername { username, reply } => {
                let result = get_user_by_username_db(&conn, &username);
                let _ = reply.send(result);
            }
            UserCmd::ListUsers { reply } => {
                let result = list_users_db(&conn);
                let _ = reply.send(result);
            }
            UserCmd::UpdateLastLogin {
                id,
                timestamp_ms,
                reply,
            } => {
                let result = update_last_login_db(&conn, &id, timestamp_ms);
                let _ = reply.send(result);
            }
            UserCmd::HasAnyUsers { reply } => {
                let result = has_any_users_db(&conn);
                let _ = reply.send(result);
            }
            UserCmd::GetAllRolePermissions { reply } => {
                let result = get_all_role_permissions_db(&conn);
                let _ = reply.send(result);
            }
            UserCmd::SetRolePermission {
                role,
                permission_key,
                value,
                reply,
            } => {
                let result = set_role_permission_db(&conn, &role, &permission_key, value);
                let _ = reply.send(result);
            }
        }
    }
}

// ----------------------------------------------------------------
// Database helper functions
// ----------------------------------------------------------------

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn role_to_str(role: &UserRole) -> &'static str {
    match role {
        UserRole::Admin => "admin",
        UserRole::Operator => "operator",
        UserRole::Viewer => "viewer",
    }
}

fn str_to_role(s: &str) -> UserRole {
    match s {
        "admin" => UserRole::Admin,
        "operator" => UserRole::Operator,
        _ => UserRole::Viewer,
    }
}

fn create_user_db(conn: &rusqlite::Connection, user: &User) -> Result<User, UserError> {
    conn.execute(
        "INSERT INTO user (id, username, display_name, role, password_hash, created_ms, last_login_ms, disabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            user.id,
            user.username,
            user.display_name,
            role_to_str(&user.role),
            user.password_hash,
            user.created_ms,
            user.last_login_ms,
            user.disabled as i32,
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            UserError::UsernameExists
        } else {
            UserError::Db(e.to_string())
        }
    })?;
    get_user_db(conn, &user.id)
}

fn update_user_db(
    conn: &rusqlite::Connection,
    id: &str,
    display_name: &str,
    role: &UserRole,
    disabled: bool,
) -> Result<(), UserError> {
    let rows = conn
        .execute(
            "UPDATE user SET display_name = ?1, role = ?2, disabled = ?3 WHERE id = ?4",
            rusqlite::params![display_name, role_to_str(role), disabled as i32, id],
        )
        .map_err(|e| UserError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(UserError::NotFound);
    }
    Ok(())
}

fn update_password_db(
    conn: &rusqlite::Connection,
    id: &str,
    password_hash: &str,
) -> Result<(), UserError> {
    let rows = conn
        .execute(
            "UPDATE user SET password_hash = ?1 WHERE id = ?2",
            rusqlite::params![password_hash, id],
        )
        .map_err(|e| UserError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(UserError::NotFound);
    }
    Ok(())
}

fn delete_user_db(conn: &rusqlite::Connection, id: &str) -> Result<(), UserError> {
    let rows = conn
        .execute("DELETE FROM user WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| UserError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(UserError::NotFound);
    }
    Ok(())
}

fn get_user_db(conn: &rusqlite::Connection, id: &str) -> Result<User, UserError> {
    conn.query_row(
        "SELECT id, username, display_name, role, password_hash, created_ms, last_login_ms, disabled FROM user WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                role: str_to_role(&row.get::<_, String>(3)?),
                password_hash: row.get(4)?,
                created_ms: row.get(5)?,
                last_login_ms: row.get(6)?,
                disabled: row.get::<_, i32>(7)? != 0,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => UserError::NotFound,
        other => UserError::Db(other.to_string()),
    })
}

fn get_user_by_username_db(conn: &rusqlite::Connection, username: &str) -> Result<User, UserError> {
    conn.query_row(
        "SELECT id, username, display_name, role, password_hash, created_ms, last_login_ms, disabled FROM user WHERE username = ?1",
        rusqlite::params![username],
        |row| {
            Ok(User {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                role: str_to_role(&row.get::<_, String>(3)?),
                password_hash: row.get(4)?,
                created_ms: row.get(5)?,
                last_login_ms: row.get(6)?,
                disabled: row.get::<_, i32>(7)? != 0,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => UserError::NotFound,
        other => UserError::Db(other.to_string()),
    })
}

fn list_users_db(conn: &rusqlite::Connection) -> Vec<User> {
    let mut stmt = conn
        .prepare("SELECT id, username, display_name, role, password_hash, created_ms, last_login_ms, disabled FROM user ORDER BY username")
        .unwrap();
    stmt.query_map([], |row| {
        Ok(User {
            id: row.get(0)?,
            username: row.get(1)?,
            display_name: row.get(2)?,
            role: str_to_role(&row.get::<_, String>(3)?),
            password_hash: row.get(4)?,
            created_ms: row.get(5)?,
            last_login_ms: row.get(6)?,
            disabled: row.get::<_, i32>(7)? != 0,
        })
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

fn update_last_login_db(
    conn: &rusqlite::Connection,
    id: &str,
    timestamp_ms: i64,
) -> Result<(), UserError> {
    let rows = conn
        .execute(
            "UPDATE user SET last_login_ms = ?1 WHERE id = ?2",
            rusqlite::params![timestamp_ms, id],
        )
        .map_err(|e| UserError::Db(e.to_string()))?;
    if rows == 0 {
        return Err(UserError::NotFound);
    }
    Ok(())
}

fn has_any_users_db(conn: &rusqlite::Connection) -> bool {
    conn.query_row("SELECT COUNT(*) FROM user", [], |row| row.get::<_, i64>(0))
        .map(|c| c > 0)
        .unwrap_or(false)
}

fn get_all_role_permissions_db(conn: &rusqlite::Connection) -> AllRolePermissions {
    let mut all = AllRolePermissions::default();
    let mut stmt = conn
        .prepare("SELECT role, permission_key, allowed FROM role_permission")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
            ))
        })
        .unwrap();
    for (role_str, key, allowed) in rows.flatten() {
        let role = str_to_role(&role_str);
        if let Some(perm) = Permission::from_key(&key) {
            all.for_role_mut(&role).set(perm, allowed != 0);
        }
    }
    all
}

fn set_role_permission_db(
    conn: &rusqlite::Connection,
    role: &UserRole,
    permission_key: &str,
    value: bool,
) -> Result<(), UserError> {
    conn.execute(
        "INSERT OR REPLACE INTO role_permission (role, permission_key, allowed) VALUES (?1, ?2, ?3)",
        rusqlite::params![role_to_str(role), permission_key, value as i32],
    )
    .map_err(|e| UserError::Db(e.to_string()))?;
    Ok(())
}

// ----------------------------------------------------------------
// Tests
// ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(path: &str) -> UserStore {
        let db_path = std::path::PathBuf::from(path);
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
        }
        start_user_store_with_path(&db_path)
    }

    fn make_user(id: &str, username: &str, role: UserRole) -> User {
        User {
            id: id.to_string(),
            username: username.to_string(),
            display_name: username.to_string(),
            role,
            password_hash: auth::hash_password("password123").unwrap(),
            created_ms: now_ms(),
            last_login_ms: None,
            disabled: false,
        }
    }

    #[tokio::test]
    async fn user_crud() {
        let store = test_store("/tmp/test_user_crud.db");

        let user = make_user("u1", "admin", UserRole::Admin);
        let created = store.create_user(user).await.unwrap();
        assert_eq!(created.username, "admin");

        // Read
        let fetched = store.get_user("u1").await.unwrap();
        assert_eq!(fetched.username, "admin");

        // Read by username
        let by_name = store.get_user_by_username("admin").await.unwrap();
        assert_eq!(by_name.id, "u1");

        // Update
        store
            .update_user("u1", "Administrator", UserRole::Admin, false)
            .await
            .unwrap();
        let updated = store.get_user("u1").await.unwrap();
        assert_eq!(updated.display_name, "Administrator");

        // List
        let users = store.list_users().await;
        assert_eq!(users.len(), 1);

        // Delete
        store.delete_user("u1").await.unwrap();
        assert!(store.get_user("u1").await.is_err());

        std::fs::remove_file("/tmp/test_user_crud.db").ok();
    }

    #[tokio::test]
    async fn has_any_users_check() {
        let store = test_store("/tmp/test_user_has_any.db");

        assert!(!store.has_any_users().await);

        let user = make_user("u1", "admin", UserRole::Admin);
        store.create_user(user).await.unwrap();
        assert!(store.has_any_users().await);

        std::fs::remove_file("/tmp/test_user_has_any.db").ok();
    }

    #[tokio::test]
    async fn duplicate_username() {
        let store = test_store("/tmp/test_user_dup.db");

        let u1 = make_user("u1", "admin", UserRole::Admin);
        store.create_user(u1).await.unwrap();

        let u2 = make_user("u2", "admin", UserRole::Operator);
        let result = store.create_user(u2).await;
        assert!(matches!(result, Err(UserError::UsernameExists)));

        std::fs::remove_file("/tmp/test_user_dup.db").ok();
    }

    #[tokio::test]
    async fn authenticate_success() {
        let store = test_store("/tmp/test_user_auth.db");

        let user = make_user("u1", "admin", UserRole::Admin);
        store.create_user(user).await.unwrap();

        let authed = store.authenticate("admin", "password123").await.unwrap();
        assert_eq!(authed.username, "admin");

        // last_login_ms is updated asynchronously; re-fetch to verify
        let refetched = store.get_user("u1").await.unwrap();
        assert!(refetched.last_login_ms.is_some());

        std::fs::remove_file("/tmp/test_user_auth.db").ok();
    }

    #[tokio::test]
    async fn authenticate_wrong_password() {
        let store = test_store("/tmp/test_user_auth_fail.db");

        let user = make_user("u1", "admin", UserRole::Admin);
        store.create_user(user).await.unwrap();

        let result = store.authenticate("admin", "wrong").await;
        assert!(matches!(result, Err(AuthError::InvalidCredentials)));

        std::fs::remove_file("/tmp/test_user_auth_fail.db").ok();
    }

    #[tokio::test]
    async fn authenticate_disabled_user() {
        let store = test_store("/tmp/test_user_auth_disabled.db");

        let mut user = make_user("u1", "admin", UserRole::Admin);
        user.disabled = true;
        store.create_user(user).await.unwrap();

        let result = store.authenticate("admin", "password123").await;
        assert!(matches!(result, Err(AuthError::UserDisabled)));

        std::fs::remove_file("/tmp/test_user_auth_disabled.db").ok();
    }

    #[tokio::test]
    async fn update_password() {
        let store = test_store("/tmp/test_user_update_pw.db");

        let user = make_user("u1", "admin", UserRole::Admin);
        store.create_user(user).await.unwrap();

        let new_hash = auth::hash_password("newpassword").unwrap();
        store.update_password("u1", &new_hash).await.unwrap();

        let authed = store.authenticate("admin", "newpassword").await.unwrap();
        assert_eq!(authed.username, "admin");

        std::fs::remove_file("/tmp/test_user_update_pw.db").ok();
    }
}
