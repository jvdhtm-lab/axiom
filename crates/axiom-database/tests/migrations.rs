mod common;

use std::path::PathBuf;

use axiom_database::migrations::{
    applied_migrations, apply_plan, load_migrations, mark_workspace, plan_migrations, ApplyOptions,
    MigrationLock, MigrationWorkspace,
};
use axiom_database::sqlite::Connection;
use axiom_database::value::Params;
use axiom_database::{Actor, AuditLog, DatabaseSpec, DbError, MigrationApproval, MigrationPlan};

use common::TempDir;

struct Fixture {
    #[allow(dead_code)]
    temp: TempDir,
    root: PathBuf,
    migrations: PathBuf,
    spec: DatabaseSpec,
    audit: AuditLog,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let temp = TempDir::new(label);
        let root = temp.path().to_path_buf();
        mark_workspace(&root).expect("marker");
        let migrations = root.join("db").join("migrations");
        std::fs::create_dir_all(&migrations).expect("migrations dir");
        let spec = DatabaseSpec::new("app", root.join("app.sqlite"))
            .with_tables(&["users"])
            .without_write_approval()
            .with_migrations(migrations.clone());
        Fixture {
            temp,
            root,
            migrations,
            spec,
            audit: AuditLog::memory(),
        }
    }

    fn write(&self, version: u32, name: &str, sql: &str) {
        std::fs::write(
            self.migrations.join(format!("{version:04}_{name}.sql")),
            sql,
        )
        .expect("write migration");
    }

    fn workspace(&self) -> MigrationWorkspace {
        MigrationWorkspace::open(&self.root).expect("workspace")
    }

    fn plan(&self) -> MigrationPlan {
        let connection = Connection::open(&self.spec.path, false).expect("open");
        plan_migrations(&self.migrations, &connection).expect("plan")
    }

    fn apply(&self, plan: &MigrationPlan, options: ApplyOptions<'_>) -> Result<(), DbError> {
        let workspace = self.workspace();
        apply_plan(&self.spec, plan, &workspace, &options, &self.audit).map(|_| ())
    }

    fn options(&self) -> ApplyOptions<'_> {
        ApplyOptions {
            allow_destructive: false,
            approval: None,
            actor: Actor::system("run"),
        }
    }

    fn read(&self) -> Connection {
        Connection::open(&self.spec.path, true).expect("read")
    }
}

#[test]
fn plans_and_applies_reviewed_migrations_in_order() {
    let fixture = Fixture::new("mig-apply");
    fixture.write(
        1,
        "create_users",
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
    );
    fixture.write(2, "add_email", "ALTER TABLE users ADD COLUMN email TEXT;");

    let plan = fixture.plan();
    assert_eq!(plan.pending.len(), 2);
    fixture.apply(&plan, fixture.options()).expect("apply");

    let read = fixture.read();
    let rows = read
        .query(
            "SELECT name FROM pragma_table_info('users') ORDER BY cid",
            &Params::new(),
            10,
            std::time::Duration::from_secs(1),
        )
        .expect("columns");
    let names: Vec<String> = rows
        .iter()
        .filter_map(|row| match row.get("name") {
            Some(axiom_database::Value::Text(name)) => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["id", "name", "email"]);

    let applied = applied_migrations(&read).expect("history");
    assert_eq!(applied.len(), 2);

    let again = fixture.plan();
    assert!(again.pending.is_empty());
    fixture.apply(&again, fixture.options()).expect("no-op");
}

#[test]
fn checksum_divergence_is_refused() {
    let fixture = Fixture::new("mig-diverge");
    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    fixture.apply(&plan, fixture.options()).expect("apply");

    fixture.write(
        1,
        "create_users",
        "CREATE TABLE users (id INTEGER, extra TEXT);",
    );
    let connection = Connection::open(&fixture.spec.path, false).expect("open");
    assert_eq!(
        plan_migrations(&fixture.migrations, &connection).unwrap_err(),
        DbError::MigrationDiverged("1".to_string())
    );
}

#[test]
fn destructive_migrations_require_flag_and_approval() {
    let fixture = Fixture::new("mig-destructive");
    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    fixture.apply(&plan, fixture.options()).expect("apply");

    fixture.write(2, "drop_users", "DROP TABLE users;");
    let plan = fixture.plan();
    assert_eq!(
        fixture.apply(&plan, fixture.options()).unwrap_err(),
        DbError::DestructiveRefused
    );
    let flagged = ApplyOptions {
        allow_destructive: true,
        approval: None,
        actor: Actor::system("run"),
    };
    assert_eq!(
        fixture.apply(&plan, flagged).unwrap_err(),
        DbError::DestructiveRefused
    );

    let approval =
        MigrationApproval::issue(&plan, "app", "reviewer", axiom_database::unix_now() + 60);
    let approved = ApplyOptions {
        allow_destructive: true,
        approval: Some(&approval),
        actor: Actor::system("run"),
    };
    fixture.apply(&plan, approved).expect("approved drop");

    let read = fixture.read();
    let rows = read
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'users'",
            &Params::new(),
            1,
            std::time::Duration::from_secs(1),
        )
        .expect("tables");
    assert!(rows.is_empty());
}

#[test]
fn scratch_failure_leaves_the_target_unchanged() {
    let fixture = Fixture::new("mig-scratch");
    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    fixture.apply(&plan, fixture.options()).expect("apply");

    fixture.write(2, "broken", "INSERT INTO missing_table VALUES (1);");
    let plan = fixture.plan();
    assert!(fixture.apply(&plan, fixture.options()).is_err());

    let read = fixture.read();
    let applied = applied_migrations(&read).expect("history");
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].0, 1);
}

#[test]
fn plan_changes_after_planning_are_refused() {
    let fixture = Fixture::new("mig-changed");
    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    fixture.write(2, "add_email", "ALTER TABLE users ADD COLUMN email TEXT;");
    assert_eq!(
        fixture.apply(&plan, fixture.options()).unwrap_err(),
        DbError::MigrationDiverged("plan changed".to_string())
    );
}

#[test]
fn locks_are_exclusive_and_released_on_drop() {
    let fixture = Fixture::new("mig-lock");
    let lock = MigrationLock::acquire(&fixture.root).expect("first lock");
    assert_eq!(
        MigrationLock::acquire(&fixture.root).unwrap_err(),
        DbError::MigrationLocked
    );

    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    assert_eq!(
        fixture.apply(&plan, fixture.options()).unwrap_err(),
        DbError::MigrationLocked
    );
    drop(lock);
    fixture
        .apply(&plan, fixture.options())
        .expect("after unlock");
}

#[test]
fn migration_workspaces_must_be_marked_and_contain_the_directory() {
    let temp = TempDir::new("mig-workspace");
    assert!(MigrationWorkspace::open(temp.path()).is_err());

    let fixture = Fixture::new("mig-outside");
    let outside_temp = TempDir::new("mig-outside-dir");
    let outside = outside_temp.path().to_path_buf();
    let mut spec = fixture.spec.clone();
    spec.migrations_dir = Some(outside.clone());
    fixture.write(1, "create_users", "CREATE TABLE users (id INTEGER);");
    let plan = fixture.plan();
    let workspace = fixture.workspace();
    assert!(matches!(
        apply_plan(
            &spec,
            &plan,
            &workspace,
            &ApplyOptions {
                allow_destructive: false,
                approval: None,
                actor: Actor::system("run"),
            },
            &fixture.audit
        ),
        Err(DbError::MigrationWorkspace(_))
    ));
}

#[test]
fn invalid_and_duplicate_filenames_are_refused() {
    let fixture = Fixture::new("mig-names");
    std::fs::write(fixture.migrations.join("not_a_migration.sql"), "SELECT 1;").ok();
    assert!(load_migrations(&fixture.migrations).is_err());

    fixture.write(1, "one", "CREATE TABLE users (id INTEGER);");
    fixture.write(1, "two", "CREATE TABLE tasks (id INTEGER);");
    assert!(load_migrations(&fixture.migrations).is_err());
}
