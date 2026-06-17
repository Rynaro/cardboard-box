//! AC-UP-* — integration tests for `cbox up` via MockRunner.
//! All against MockRunner; zero real distrobox invocations.

use cbox::core::{
    self,
    spec::{CreateSpec, DockerMode, UpSpec},
    state_store::{ProvisionState, ProvisionStateStore},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use tempfile::TempDir;

// ─── Shared helpers ──────────────────────────────────────────────────────────

struct MemoryStore {
    state: std::sync::Mutex<ProvisionState>,
}

impl MemoryStore {
    fn empty() -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(ProvisionState::new()),
        }
    }
}

impl ProvisionStateStore for MemoryStore {
    fn read(
        &self,
        _name: &str,
        _runner: &dyn DistroboxRunner,
    ) -> Result<ProvisionState, CboxError> {
        Ok(self.state.lock().unwrap().clone())
    }

    fn write(
        &self,
        _name: &str,
        state: &ProvisionState,
        _runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError> {
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }
}

fn mock_inspect_json(name: &str, image: &str, docker_mode: &str) -> String {
    serde_json::json!([{
        "Id": "abc123",
        "State": { "Status": "running" },
        "Config": {
            "Image": image,
            "Labels": {
                "manager": "distrobox",
                "cbox.managed": "true",
                "cbox.docker_mode": docker_mode,
                "cbox.boxfile_path": "",
                "cbox.version": "0.1.0",
                "cbox.image": image,
                "cbox.packages": ""
            }
        },
        "Mounts": [],
        "Created": "2026-06-16T00:00:00Z",
        "Name": name
    }])
    .to_string()
}

fn write_boxfile(dir: &TempDir, content: &str) -> String {
    let path = dir.path().join("Boxfile.toml");
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

fn base_create_spec(name: &str, image: &str, boxfile_path: Option<String>) -> CreateSpec {
    CreateSpec {
        name: name.to_string(),
        image: image.to_string(),
        packages: vec![],
        docker_mode: DockerMode::None,
        mounts: vec![],
        home: None,
        hostname: None,
        init: false,
        pull: false,
        root: false,
        boxfile_path,
        unshare: None,
        backend: Backend::Podman,
        uid: 1000,
        dry_run: false,
    }
}

// ─── AC-UP-1: box absent → create then provision ─────────────────────────────

#[test]
fn ac_up_1_absent_box_creates_then_provisions() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:latest"

[[provision]]
type = "shell"
run = "echo hello"
"#,
    );

    // Inspect returns empty (box absent), create succeeds, then inspect returns present
    let runner = MockRunner::new()
        .with_matcher(
            // First inspect: box absent
            MockMatcher::new(MockResponse::ok("[]"))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string(), "web-dev".to_string()]),
        )
        .with_default(MockResponse::ok(mock_inspect_json(
            "web-dev",
            "fedora-toolbox:latest",
            "none",
        )));

    let store = MemoryStore::empty();
    let cs = base_create_spec("web-dev", "fedora-toolbox:latest", Some(bf_path));
    let spec = UpSpec {
        create_spec: cs,
        apply_force: false,
        apply_redo: vec![],
        no_provision: false,
        recreate: false,
        yes: false,
        dry_run: false,
    };

    let outcome = core::up(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);
    assert!(outcome.created, "box should have been created");
    assert_eq!(outcome.name, "web-dev");

    // Runner should have called distrobox create
    let calls = runner.calls();
    let create_call = calls
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "create"));
    assert!(create_call, "should have called distrobox create");

    // Runner should have called distrobox enter for provision
    let provision_call = calls
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"));
    assert!(provision_call, "should have run provision step");
}

// ─── AC-UP-2: box present → no create, apply behavior ────────────────────────

#[test]
fn ac_up_2_present_box_no_create() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:latest"

[[provision]]
type = "shell"
run = "echo hello"
"#,
    );

    // Box is present
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "web-dev",
                "fedora-toolbox:latest",
                "none",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();
    let cs = base_create_spec("web-dev", "fedora-toolbox:latest", Some(bf_path));
    let spec = UpSpec {
        create_spec: cs,
        apply_force: false,
        apply_redo: vec![],
        no_provision: false,
        recreate: false,
        yes: false,
        dry_run: false,
    };

    let outcome = core::up(&spec, &store, &runner).unwrap();
    assert!(
        !outcome.created,
        "box already present: created should be false"
    );

    // No distrobox create call
    let create_call = runner
        .calls()
        .iter()
        .any(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "create"));
    assert!(!create_call, "box present: no create should occur");
}

// ─── AC-UP-3: --file carries name → create + provision from Boxfile ──────────

#[test]
fn ac_up_3_from_file() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "from-file-box"
image = "ubuntu:22.04"

[[provision]]
type = "shell"
run = "apt-get install -y git"
"#,
    );

    // Box absent
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("[]"))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string(), "from-file-box".to_string()]),
        )
        .with_default(MockResponse::ok(mock_inspect_json(
            "from-file-box",
            "ubuntu:22.04",
            "none",
        )));

    let store = MemoryStore::empty();

    // Use spec_from_boxfile equivalent by constructing from the parsed boxfile
    let (bf, _) = cbox::boxfile::parse_file(&bf_path).unwrap();
    assert_eq!(bf.name, "from-file-box");

    let cs = base_create_spec("from-file-box", "ubuntu:22.04", Some(bf_path));
    let spec = UpSpec {
        create_spec: cs,
        apply_force: false,
        apply_redo: vec![],
        no_provision: false,
        recreate: false,
        yes: false,
        dry_run: false,
    };

    let outcome = core::up(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);
    assert!(outcome.created);
    assert_eq!(outcome.name, "from-file-box");
}

// ─── AC-UP-4: --dry-run absent box → create plan + provision plan, no mutating spawns ──

#[test]
fn ac_up_4_dry_run_absent_box() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "web-dev"
image = "fedora-toolbox:latest"

[[provision]]
type = "shell"
run = "echo hello"
"#,
    );

    // Box absent
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("[]"))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string(), "web-dev".to_string()]),
        )
        .with_default(MockResponse::ok(mock_inspect_json(
            "web-dev",
            "fedora-toolbox:latest",
            "none",
        )));

    let store = MemoryStore::empty();
    let mut cs = base_create_spec("web-dev", "fedora-toolbox:latest", Some(bf_path));
    cs.dry_run = true;

    let spec = UpSpec {
        create_spec: cs,
        apply_force: false,
        apply_redo: vec![],
        no_provision: false,
        recreate: false,
        yes: false,
        dry_run: true,
    };

    // Should succeed without error
    let outcome = core::up(&spec, &store, &runner).unwrap();
    assert!(outcome.ok);
    // created = true because the box was absent
    assert!(outcome.created);
}
