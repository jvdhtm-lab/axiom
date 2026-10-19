use axiom_database::postgres::{PostgresPolicy, SslMode};
use axiom_database::DbError;

#[test]
fn tls_is_required() {
    let policy = PostgresPolicy::default();
    assert!(matches!(
        policy.parse("postgres://app@db.example.com:5432/app"),
        Err(DbError::Config(_))
    ));
    assert!(matches!(
        policy.parse("postgres://app@db.example.com:5432/app?sslmode=disable"),
        Err(DbError::Config(_))
    ));
    let target = policy
        .parse("postgres://app:secret@db.example.com:5432/app?sslmode=verify-full")
        .expect("valid target");
    assert_eq!(target.host, "db.example.com");
    assert_eq!(target.database, "app");
    assert_eq!(target.sslmode, SslMode::VerifyFull);
    assert_eq!(target.user.as_deref(), Some("app"));
    let sanitized = target.sanitized();
    assert!(!sanitized.contains("secret"));
    assert!(sanitized.contains("sslmode=verify-full"));
}

#[test]
fn private_and_metadata_targets_are_refused_by_default() {
    let policy = PostgresPolicy::default();
    for url in [
        "postgres://app@localhost/app?sslmode=require",
        "postgres://app@127.0.0.1/app?sslmode=require",
        "postgres://app@10.0.0.5/app?sslmode=require",
        "postgres://app@192.168.1.10/app?sslmode=require",
        "postgres://app@169.254.169.254/app?sslmode=require",
        "postgres://app@[::1]/app?sslmode=require",
        "postgres://app@metadata.google.internal/app?sslmode=require",
    ] {
        assert!(
            matches!(policy.parse(url), Err(DbError::Config(_))),
            "{url} should be refused"
        );
    }
}

#[test]
fn explicit_private_network_opt_in_is_supported() {
    let policy = PostgresPolicy {
        allow_private_network: true,
    };
    let target = policy
        .parse("postgres://app@127.0.0.1:5433/app?sslmode=require")
        .expect("opt-in");
    assert_eq!(target.port, 5433);
    assert_eq!(target.sslmode, SslMode::Require);
}

#[test]
fn release_postgres_url_is_validated_when_present() {
    let Ok(url) = std::env::var("AXIOM_TEST_POSTGRES_URL") else {
        return;
    };
    let policy = PostgresPolicy {
        allow_private_network: true,
    };
    policy
        .parse(&url)
        .expect("AXIOM_TEST_POSTGRES_URL is valid");
}
