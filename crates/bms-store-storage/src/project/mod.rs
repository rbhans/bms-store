use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Metadata stored inside a project directory as `project.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_ms: i64,
    pub version: String,
}

/// A reference to a project stored in the global registry (`projects.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub last_opened_ms: i64,
}

/// The global registry of known projects.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectRegistry {
    pub projects: Vec<ProjectRef>,
}

/// All derived paths for a project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub scenario: PathBuf,
    pub profiles_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl ProjectPaths {
    pub fn from_root(root: PathBuf) -> Self {
        let scenario = root.join("scenario.json");
        let profiles_dir = root.join("profiles");
        let data_dir = root.join("data");
        Self {
            root,
            scenario,
            profiles_dir,
            data_dir,
        }
    }

    pub fn db_path(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }
}

/// Returns the OpenCrate home directory (`~/.opencrate/`).
/// Can be overridden with the `OPENCRATE_HOME` env var (used in tests).
pub fn opencrate_home() -> PathBuf {
    if let Ok(custom) = std::env::var("OPENCRATE_HOME") {
        return PathBuf::from(custom);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".opencrate")
}

/// Path to the global project registry file.
fn registry_path() -> PathBuf {
    opencrate_home().join("projects.json")
}

/// Path to the projects directory.
fn projects_dir() -> PathBuf {
    opencrate_home().join("projects")
}

/// Load the project registry from disk.
/// Returns empty default only if the file does not exist.
/// Returns an error if the file exists but cannot be read or parsed,
/// preventing silent data loss from a corrupt registry.
pub fn load_registry() -> Result<ProjectRegistry, String> {
    let path = registry_path();
    if !path.exists() {
        return Ok(ProjectRegistry::default());
    }
    let data = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    serde_json::from_str(&data).map_err(|e| format!("Corrupt registry {}: {e}", path.display()))
}

/// Save the project registry to disk.
pub fn save_registry(reg: &ProjectRegistry) -> std::io::Result<()> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string_pretty(reg).map_err(std::io::Error::other)?;
    std::fs::write(&path, data)
}

/// Create a new project, optionally copying from a template scenario directory.
///
/// If `template_scenario` and `template_profiles` are provided, the scenario and profile
/// files are copied from those paths. Otherwise, a minimal empty scenario is created.
pub fn create_project(
    name: &str,
    desc: &str,
    template_scenario: Option<&Path>,
    template_profiles: Option<&Path>,
) -> Result<ProjectRef, Box<dyn std::error::Error>> {
    let id = uuid::Uuid::new_v4().to_string();
    let project_dir = projects_dir().join(&id);
    let paths = ProjectPaths::from_root(project_dir.clone());

    // Create directory structure
    std::fs::create_dir_all(&paths.profiles_dir)?;
    std::fs::create_dir_all(&paths.data_dir)?;

    // Copy or create scenario
    if let Some(src) = template_scenario {
        if src.exists() {
            std::fs::copy(src, &paths.scenario)?;
        } else {
            write_minimal_scenario(&paths.scenario)?;
        }
    } else {
        write_minimal_scenario(&paths.scenario)?;
    }

    // Copy template profiles if provided
    if let Some(profiles_src) = template_profiles {
        if profiles_src.is_dir() {
            for entry in std::fs::read_dir(profiles_src)? {
                let entry = entry?;
                let file_path = entry.path();
                if file_path.extension().map(|e| e == "json").unwrap_or(false) {
                    let dest = paths.profiles_dir.join(entry.file_name());
                    std::fs::copy(&file_path, &dest)?;
                }
            }
        }
    }

    // Write project metadata
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let meta = ProjectMeta {
        id: id.clone(),
        name: name.to_string(),
        description: desc.to_string(),
        created_ms: now_ms,
        version: "0.1.0".to_string(),
    };
    let meta_json = serde_json::to_string_pretty(&meta)?;
    std::fs::write(project_dir.join("project.json"), meta_json)?;

    // Register in global registry
    let project_ref = ProjectRef {
        id,
        name: name.to_string(),
        path: project_dir.clone(),
        last_opened_ms: now_ms,
    };
    let mut reg = load_registry()?;
    reg.projects.push(project_ref.clone());
    if let Err(e) = save_registry(&reg) {
        // Clean up the directory we just created to avoid orphans
        let _ = std::fs::remove_dir_all(&project_dir);
        return Err(Box::new(e));
    }

    Ok(project_ref)
}

/// Delete a project: delete directory first, then remove from registry.
/// This ordering ensures a failed directory deletion doesn't leave the
/// project invisible but still on disk.
pub fn delete_project(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut reg = load_registry()?;
    let pos = reg.projects.iter().position(|p| p.id == id);
    if let Some(idx) = pos {
        let project_path = reg.projects[idx].path.clone();
        // Delete directory first — if this fails, registry still shows the project
        if project_path.exists() {
            std::fs::remove_dir_all(&project_path)?;
        }
        // Only remove from registry after directory is gone
        reg.projects.remove(idx);
        save_registry(&reg)?;
    }
    Ok(())
}

/// Rename a project: updates both `project.json` and the global registry.
pub fn rename_project(
    id: &str,
    new_name: &str,
    project_root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Update project.json inside the project directory
    let meta_path = project_root.join("project.json");
    if meta_path.exists() {
        let data = std::fs::read_to_string(&meta_path)?;
        let mut meta: ProjectMeta = serde_json::from_str(&data)?;
        meta.name = new_name.to_string();
        let json = serde_json::to_string_pretty(&meta)?;
        std::fs::write(&meta_path, json)?;
    }

    // Update global registry
    let mut reg = load_registry()?;
    if let Some(entry) = reg.projects.iter_mut().find(|p| p.id == id) {
        entry.name = new_name.to_string();
    }
    save_registry(&reg)?;

    Ok(())
}

/// Export a project to a `.ocrate` archive (tar+gzip).
pub fn export_project(id: &str, dest: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let reg = load_registry()?;
    let project_ref = reg
        .projects
        .iter()
        .find(|p| p.id == id)
        .ok_or("Project not found")?;

    let file = std::fs::File::create(dest)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);

    // Walk the project directory, skipping WAL/SHM files
    add_dir_to_archive(&mut archive, &project_ref.path, &project_ref.path)?;

    archive.into_inner()?.finish()?;
    Ok(())
}

/// Import a project from a `.ocrate` archive.
pub fn import_project(archive_path: &Path) -> Result<ProjectRef, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(archive_path)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // Extract to a temp directory first to read the project.json
    let new_id = uuid::Uuid::new_v4().to_string();
    let dest_dir = projects_dir().join(&new_id);
    std::fs::create_dir_all(&dest_dir)?;

    archive.unpack(&dest_dir)?;

    // Read the original project metadata
    let meta_path = dest_dir.join("project.json");
    let mut meta: ProjectMeta = if meta_path.exists() {
        let data = std::fs::read_to_string(&meta_path)?;
        serde_json::from_str(&data)?
    } else {
        // Fallback if no project.json in archive
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        ProjectMeta {
            id: new_id.clone(),
            name: "Imported Project".to_string(),
            description: String::new(),
            created_ms: now_ms,
            version: "0.1.0".to_string(),
        }
    };

    // Reassign UUID
    meta.id = new_id.clone();
    let meta_json = serde_json::to_string_pretty(&meta)?;
    std::fs::write(&meta_path, meta_json)?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let project_ref = ProjectRef {
        id: new_id,
        name: meta.name,
        path: dest_dir.clone(),
        last_opened_ms: now_ms,
    };
    let mut reg = load_registry()?;
    reg.projects.push(project_ref.clone());
    if let Err(e) = save_registry(&reg) {
        // Clean up unpacked directory to avoid orphans
        let _ = std::fs::remove_dir_all(&dest_dir);
        return Err(Box::new(e));
    }

    Ok(project_ref)
}

/// Migrate legacy data from CWD into a new project.
///
/// Checks if `data/` exists in the current working directory.
/// If so, creates a "Default" project and copies `data/`, `scenarios/`, `profiles/` into it.
pub fn migrate_legacy_if_needed() -> Option<ProjectRef> {
    let cwd = std::env::current_dir().ok()?;
    let legacy_data = cwd.join("data");
    if !legacy_data.exists() {
        return None;
    }

    // Don't migrate if registry already has projects or is corrupt
    let reg = match load_registry() {
        Ok(r) => r,
        Err(_) => return None, // corrupt registry — don't overwrite
    };
    if !reg.projects.is_empty() {
        return None;
    }

    let scenario_src = cwd.join("scenarios").join("small-office.json");
    let profiles_src = cwd.join("profiles");

    let project_ref = create_project(
        "Default",
        "Migrated from legacy layout",
        if scenario_src.exists() {
            Some(scenario_src.as_path())
        } else {
            None
        },
        if profiles_src.exists() {
            Some(profiles_src.as_path())
        } else {
            None
        },
    )
    .ok()?;

    // Copy data/*.db files (skip WAL/SHM)
    let dest_data = ProjectPaths::from_root(project_ref.path.clone()).data_dir;
    if let Ok(entries) = std::fs::read_dir(&legacy_data) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "db" {
                    let dest = dest_data.join(entry.file_name());
                    let _ = std::fs::copy(&path, &dest);
                }
            }
        }
    }

    Some(project_ref)
}

/// Update last_opened_ms for a project in the registry.
pub fn touch_project(id: &str) {
    let mut reg = match load_registry() {
        Ok(r) => r,
        Err(_) => return,
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    if let Some(p) = reg.projects.iter_mut().find(|p| p.id == id) {
        p.last_opened_ms = now_ms;
    }
    let _ = save_registry(&reg);
}

/// Load the ProjectMeta from a project directory.
pub fn load_project_meta(project_dir: &Path) -> Result<ProjectMeta, Box<dyn std::error::Error>> {
    let meta_path = project_dir.join("project.json");
    let data = std::fs::read_to_string(&meta_path)?;
    let meta: ProjectMeta = serde_json::from_str(&data)?;
    Ok(meta)
}

/// Validate that a project directory is openable.
/// Checks that the root dir exists and contains a parseable scenario.json.
pub fn validate_project_path(paths: &ProjectPaths) -> Result<(), String> {
    if !paths.root.exists() {
        return Err(format!(
            "Project directory does not exist: {}",
            paths.root.display()
        ));
    }
    if !paths.scenario.exists() {
        return Err(format!("Missing scenario.json in {}", paths.root.display()));
    }
    // Try parsing the scenario to catch JSON errors early
    let data = std::fs::read_to_string(&paths.scenario)
        .map_err(|e| format!("Cannot read scenario.json: {e}"))?;
    let _: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| format!("Invalid scenario.json: {e}"))?;
    Ok(())
}

// --- Helpers ---

fn write_minimal_scenario(path: &Path) -> std::io::Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let scenario = serde_json::json!({
        "scenario": {
            "id": id,
            "name": "Empty Project",
            "description": "A new OpenCrate project"
        },
        "devices": []
    });
    let data = serde_json::to_string_pretty(&scenario).unwrap();
    std::fs::write(path, data)
}

fn add_dir_to_archive<W: std::io::Write>(
    archive: &mut tar::Builder<W>,
    base: &Path,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(base)?;

        if path.is_dir() {
            add_dir_to_archive(archive, base, &path)?;
        } else {
            // Skip SQLite WAL/SHM files
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if ext_str == "db-wal" || ext_str == "db-shm" {
                    continue;
                }
            }
            // Also check for hyphenated extensions like foo.db-wal
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.ends_with("-wal") || name.ends_with("-shm") {
                continue;
            }
            archive.append_path_with_name(&path, relative)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Serialize all project tests since they share the OPENCRATE_HOME env var.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: create a temp dir with its own registry, isolated from ~/.opencrate.
    fn with_temp_home<F: FnOnce(PathBuf)>(f: F) {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("opencrate-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        // Override OPENCRATE_HOME so opencrate_home() points to our temp dir
        unsafe {
            std::env::set_var("OPENCRATE_HOME", &dir);
        }
        f(dir.clone());
        unsafe {
            std::env::remove_var("OPENCRATE_HOME");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_project_produces_valid_scenario() {
        with_temp_home(|_home| {
            let proj = create_project("Test", "A test project", None, None).unwrap();
            let paths = ProjectPaths::from_root(proj.path.clone());

            // scenario.json must exist and be parseable
            assert!(paths.scenario.exists());
            let data = fs::read_to_string(&paths.scenario).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&data).unwrap();
            // Must have scenario.id
            assert!(parsed["scenario"]["id"].is_string());
            assert!(!parsed["scenario"]["id"].as_str().unwrap().is_empty());

            // project.json must exist
            assert!(paths.root.join("project.json").exists());

            // data/ and profiles/ dirs must exist
            assert!(paths.data_dir.exists());
            assert!(paths.profiles_dir.exists());

            // Registry must contain the project
            let reg = load_registry().unwrap();
            assert_eq!(reg.projects.len(), 1);
            assert_eq!(reg.projects[0].id, proj.id);
        });
    }

    #[test]
    fn validate_project_path_catches_missing_dir() {
        let paths = ProjectPaths::from_root(PathBuf::from("/nonexistent/path"));
        assert!(validate_project_path(&paths).is_err());
    }

    #[test]
    fn validate_project_path_catches_missing_scenario() {
        let dir = std::env::temp_dir().join(format!("opencrate-val-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let paths = ProjectPaths::from_root(dir.clone());
        assert!(validate_project_path(&paths).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_registry_returns_error_on_corrupt_json() {
        with_temp_home(|home| {
            // OPENCRATE_HOME is set to `home`, so projects.json is at home/projects.json
            let reg_path = home.join("projects.json");
            fs::write(&reg_path, "not valid json{{{").unwrap();
            let result = load_registry();
            assert!(result.is_err());
        });
    }

    #[test]
    fn delete_project_removes_from_registry_and_disk() {
        with_temp_home(|_home| {
            let proj = create_project("ToDelete", "", None, None).unwrap();
            let proj_path = proj.path.clone();
            assert!(proj_path.exists());

            delete_project(&proj.id).unwrap();
            assert!(!proj_path.exists());
            let reg = load_registry().unwrap();
            assert!(reg.projects.is_empty());
        });
    }

    #[test]
    fn export_import_roundtrip() {
        with_temp_home(|home| {
            let proj = create_project("Export Test", "roundtrip", None, None).unwrap();
            let archive_path = home.join("test.ocrate");
            export_project(&proj.id, &archive_path).unwrap();
            assert!(archive_path.exists());

            let imported = import_project(&archive_path).unwrap();
            assert_ne!(imported.id, proj.id); // new UUID assigned
            assert_eq!(imported.name, "Export Test");

            let reg = load_registry().unwrap();
            assert_eq!(reg.projects.len(), 2);
        });
    }
}
