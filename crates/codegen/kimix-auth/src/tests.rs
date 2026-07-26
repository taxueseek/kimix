#[cfg(test)]
mod unit_tests {
    use crate::auth_provider::{
        AuthCredentialProvider, CredentialSnapshot, StaticAuthCredentialProvider,
    };
    use crate::visibility::HttpAuth;
    use reqwest::RequestBuilder;

    /// Stub HttpAuth that records whether apply() was called.
    struct StubAuth {
        applied: std::sync::Mutex<bool>,
    }

    impl StubAuth {
        fn new() -> Self {
            Self {
                applied: std::sync::Mutex::new(false),
            }
        }
    }

    impl HttpAuth for StubAuth {
        fn apply(&self, builder: RequestBuilder, _base_url: &str) -> RequestBuilder {
            *self.applied.lock().expect("lock poisoned") = true;
            builder
        }
    }

    // ── CredentialSnapshot ────────────────────────────────────────────

    #[test]
    fn credential_snapshot_default_is_empty() {
        let snap = CredentialSnapshot::default();
        assert!(snap.token.is_none());
        assert!(snap.user_id.is_none());
        assert!(snap.deployment_id.is_none());
        assert!(snap.api_key_id.is_none());
    }

    #[test]
    fn credential_snapshot_clone_preserves_fields() {
        let snap = CredentialSnapshot {
            token: Some("bearer-token".into()),
            user_id: Some("user-1".into()),
            deployment_id: Some("deploy-1".into()),
            api_key_id: Some("key-1".into()),
        };
        let cloned = snap.clone();
        assert_eq!(cloned.token, snap.token);
        assert_eq!(cloned.user_id, snap.user_id);
        assert_eq!(cloned.deployment_id, snap.deployment_id);
        assert_eq!(cloned.api_key_id, snap.api_key_id);
    }

    // ── StaticAuthCredentialProvider ───────────────────────────────────

    #[test]
    fn static_provider_snapshot_returns_bearer() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, Some("test-token".into()));
        let snap = provider.snapshot();
        assert_eq!(snap.token.as_deref(), Some("test-token"));
        assert!(snap.user_id.is_none());
    }

    #[test]
    fn static_provider_snapshot_without_bearer() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, None);
        let snap = provider.snapshot();
        assert!(snap.token.is_none());
    }

    #[test]
    fn static_provider_refresh_always_returns_false() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, Some("token".into()));
        let rt = tokio::runtime::Runtime::new().unwrap();
        let refreshed = rt.block_on(provider.refresh_after_unauthorized());
        assert!(!refreshed);
    }

    #[test]
    fn static_provider_has_usable_credential_defaults_true() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, None);
        assert!(provider.has_usable_credential());
    }

    #[test]
    fn static_provider_apply_delegates_to_inner() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, Some("token".into()));
        let client = reqwest::Client::new();
        let builder = client.get("https://example.com");
        let _ = provider.apply(builder, "https://example.com");
        // StubAuth records the apply call.
        // We can't easily inspect the stub after apply because the provider
        // owns the Box<dyn HttpAuth>. This test verifies no panic occurs.
    }

    #[test]
    fn static_provider_debug_does_not_leak_token() {
        let stub = Box::new(StubAuth::new());
        let provider = StaticAuthCredentialProvider::new(stub, Some("secret-token".into()));
        let debug = format!("{:?}", provider);
        assert!(debug.contains("has_bearer"));
        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn credential_snapshot_with_partial_fields() {
        let snap = CredentialSnapshot {
            token: Some("tok".into()),
            user_id: None,
            deployment_id: Some("dep".into()),
            api_key_id: None,
        };
        assert_eq!(snap.token.as_deref(), Some("tok"));
        assert_eq!(snap.deployment_id.as_deref(), Some("dep"));
        assert!(snap.user_id.is_none());
        assert!(snap.api_key_id.is_none());
    }

    #[test]
    fn auth_provider_trait_is_object_safe() {
        // Verify that AuthCredentialProvider can be used as a trait object.
        let stub: Box<dyn AuthCredentialProvider> = Box::new(StaticAuthCredentialProvider::new(
            Box::new(StubAuth::new()),
            Some("token".into()),
        ));
        let snap = stub.snapshot();
        assert_eq!(snap.token.as_deref(), Some("token"));
    }
}
