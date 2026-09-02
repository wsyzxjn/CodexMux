#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Paths {
        let paths = Paths::from_root(tempfile::tempdir().unwrap().keep());
        crate::secrets::save(
            &paths.credentials,
            &crate::config::Credentials {
                proxy_token: "proxy-token".into(),
                cpa_token: "cpa-token".into(),
                cpa_management_key: "management-key".into(),
            },
        )
        .unwrap();
        paths
    }

    fn cpa_local(port: u16) -> Cpa {
        Cpa {
            base_url: format!("http://127.0.0.1:{port}/v1"),
        }
    }

    #[test]
    fn extract_accepts_only_the_expected_binary_entry() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("release.tar.gz");
        fs::write(
            &archive,
            include_bytes!("../../tests/fixtures/cpa_release/mini_release.tar.gz"),
        )
        .unwrap();
        let binary = extract_binary(&archive).unwrap();
        assert_eq!(binary, b"#!/bin/sh\necho fake-cli-proxy-api\n");
    }

    #[test]
    fn downloaded_archive_is_removed_only_after_successful_install() {
        let root = tempfile::tempdir().unwrap();
        let archive = root.path().join("release.tar.gz");
        fs::write(&archive, b"archive").unwrap();

        finish_download_install(&archive, Ok(())).unwrap();
        assert!(!archive.exists());

        fs::write(&archive, b"archive").unwrap();
        assert!(finish_download_install(&archive, Err(anyhow::anyhow!("failed"))).is_err());
        assert!(archive.exists());
    }

    #[test]
    fn model_slug_parser_accepts_native_and_openai_catalog_shapes() {
        assert_eq!(
            model_slugs_from_value(&serde_json::json!({
                "models": [{"slug":"b"}, {"slug":"a"}, {"slug":"a"}]
            }))
            .unwrap(),
            ["a", "b"]
        );
        assert_eq!(
            model_slugs_from_value(&serde_json::json!({
                "data": [{"id":"model-b"}, {"id":"model-a"}]
            }))
            .unwrap(),
            ["model-a", "model-b"]
        );
    }

    #[test]
    fn direct_routes_validate_endpoint_models_and_token_isolation() {
        let paths = paths();
        let route = DirectRoute {
            base_url: "http://127.0.0.1:9000/v1".into(),
            token: "direct-token".into(),
            models: vec!["model-a".into()],
            model_aliases: BTreeMap::new(),
        };
        set_direct_routes(&paths, vec![route]).unwrap();
        let direct = direct_route_for(&paths.cpa_profiles, "model-a")
            .unwrap()
            .unwrap();
        assert_eq!(direct.base_url, "http://127.0.0.1:9000/v1");
        assert_eq!(direct.token, "direct-token");
        assert_eq!(direct.upstream_model, "model-a");

        add_direct_route_mapping(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            "local-alias".into(),
            "provider/native-model".into(),
        )
        .unwrap();
        let mapped = direct_route_for(&paths.cpa_profiles, "local-alias")
            .unwrap()
            .unwrap();
        assert_eq!(mapped.upstream_model, "provider/native-model");
        assert!(
            declared_direct_models(&paths.cpa_profiles)
                .iter()
                .any(|model| model.upstream_model == "local-alias")
        );

        let remote_http = DirectRoute {
            base_url: "http://example.com/v1".into(),
            token: "other-direct-token".into(),
            models: vec!["model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![remote_http]).is_err());

        let shared_token = DirectRoute {
            base_url: "https://example.com/v1".into(),
            token: "cpa-token".into(),
            models: vec!["model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![shared_token]).is_err());

        let prefixed_model = DirectRoute {
            base_url: "https://example.com/v1".into(),
            token: "other-direct-token".into(),
            models: vec!["cpa/model-b".into()],
            model_aliases: BTreeMap::new(),
        };
        assert!(set_direct_routes(&paths, vec![prefixed_model]).is_err());
        assert!(set_review_override(&paths.cpa_profiles, Some("cpa/model-b".into())).is_err());
    }

    #[test]
    fn direct_routes_add_and_remove_single_entries() {
        let paths = paths();
        add_direct_route(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            vec!["model-a".into()],
        )
        .unwrap();
        // Same base URL merges models; a second route is not created.
        add_direct_route(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            vec!["model-b".into()],
        )
        .unwrap();
        let routes = direct_routes(&paths);
        assert_eq!(routes.len(), 1);
        assert_eq!(
            routes[0].models,
            vec!["model-a".to_string(), "model-b".to_string()]
        );

        add_direct_route_mapping(
            &paths,
            "http://127.0.0.1:9000/v1".into(),
            "direct-token".into(),
            "local-c".into(),
            "native-c".into(),
        )
        .unwrap();
        assert_eq!(
            direct_routes(&paths)[0].model_aliases["local-c"],
            "native-c"
        );

        // Removing one model keeps the other; removing all drops the entry.
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["model-a".into()]).unwrap();
        assert_eq!(direct_routes(&paths)[0].models, vec!["model-b".to_string()]);
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["model-b".into()]).unwrap();
        assert_eq!(direct_routes(&paths).len(), 1);
        remove_direct_routes(&paths, "http://127.0.0.1:9000/v1", &["local-c".into()]).unwrap();
        assert!(direct_routes(&paths).is_empty());

        // An empty model list removes the whole entry.
        add_direct_route(
            &paths,
            "http://127.0.0.1:9001/v1".into(),
            "direct-token".into(),
            vec!["model-c".into()],
        )
        .unwrap();
        remove_direct_routes(&paths, "http://127.0.0.1:9001/v1", &[]).unwrap();
        assert!(direct_routes(&paths).is_empty());
    }

    #[test]
    fn cpa_autostart_preference_round_trips() {
        let paths = paths();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), None);
        set_cpa_autostart(&paths.cpa_profiles, true).unwrap();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), Some(true));
        set_cpa_autostart(&paths.cpa_profiles, false).unwrap();
        assert_eq!(cpa_autostart(&paths.cpa_profiles), Some(false));
    }

    #[test]
    fn managed_config_writes_loopback_port_and_token() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let text = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(text.contains(MANAGED_MARKER));
        assert!(text.contains("host: \"127.0.0.1\""));
        assert!(text.contains("port: 8317"));
        assert!(text.contains("  secret-key: \"management-key\"\n"));
        assert!(text.contains("  disable-auto-update-panel: true\n"));
        assert!(text.contains("  - \"token-a\"\n"));
        assert_eq!(config_port(&paths), 8317);
    }

    #[test]
    fn management_key_replacement_only_touches_remote_management() {
        let config = concat!(
            "# Managed by CodexMux\n",
            "remote-management:\n",
            "  allow-remote: false\n",
            "  secret-key: \"old\"\n",
            "provider:\n",
            "  secret-key: \"provider-secret\"\n",
        );
        let replaced = replace_management_key(config, "new-management-key").unwrap();
        assert!(replaced.contains("  secret-key: \"new-management-key\"\n"));
        assert!(replaced.contains("  secret-key: \"provider-secret\"\n"));
        assert!(!replaced.contains("  secret-key: \"old\"\n"));
    }

    #[test]
    fn management_connect_bootstrap_is_injected_once() {
        let html = "<!doctype html>\n<html>\n  <head>\n    <meta charset=\"UTF-8\" />\n  </head>\n</html>\n";
        let first = inject_connect_bootstrap(html);
        assert!(first.contains(CONNECT_SCRIPT_ID));
        assert!(first.contains("params.get(\"cmk\")"));
        assert_eq!(first.matches(CONNECT_SCRIPT_ID).count(), 1);

        let second = inject_connect_bootstrap(&first);
        assert_eq!(second.matches(CONNECT_SCRIPT_ID).count(), 1);
        assert_eq!(second.matches("<head>").count(), 1);
    }

    #[test]
    fn hand_edited_config_is_never_overwritten() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        fs::write(config_path(&paths), "# my own CPA config\nport: 9999\n").unwrap();
        let error = write_config(&paths, &cpa_local(8317), "token-a").unwrap_err();
        assert!(error.to_string().contains("refusing to overwrite"));
    }

    #[test]
    fn provider_section_survives_config_rewrites() {
        let paths = paths();
        // First write has no provider section yet.
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let section =
            format!("\n{PROVIDERS_HEADER}\nopenai-compatibility:\n  - name: \"example\"\n");
        let base = fs::read_to_string(config_path(&paths)).unwrap();
        // A rewrite that already contains a provider section keeps exactly one.
        let with_providers = base.replace(&format!("\n{PROVIDERS_HEADER}\n"), &section);
        fs::write(config_path(&paths), with_providers).unwrap();
        write_config(&paths, &cpa_local(9317), "token-b").unwrap();
        let rewritten = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(rewritten.contains("port: 9317"));
        assert!(rewritten.contains("token-b"));
        assert!(rewritten.contains(section.trim_end()));
        assert_eq!(rewritten.matches(PROVIDERS_HEADER).count(), 1);
    }

    #[test]
    fn import_providers_converts_toml_tables_to_cpa_yaml() {
        let toml = r#"
[[openai-compatibility]]
name = "kunbot-ris"
base-url = "https://example.com/ris/v1"

[[openai-compatibility.models]]
name = "zai-org/GLM-5.3-Flash"
alias = "glm-5.3-flash"
"#
        .trim()
        .to_owned();
        let yaml = providers_yaml(&toml).unwrap();
        assert!(yaml.starts_with("openai-compatibility:\n"));
        assert!(yaml.contains("name: \"kunbot-ris\"\n"));
        assert!(yaml.contains("base-url: \"https://example.com/ris/v1\"\n"));
        // The rendered YAML must parse back as a list of provider entries.
        let parsed =
            serde_yaml::from_str::<serde_yaml::Value>(&yaml).expect("rendered YAML is valid");
        let providers = parsed
            .get("openai-compatibility")
            .and_then(|value| value.as_sequence())
            .expect("providers parse as a list");
        assert_eq!(providers.len(), 1);
        assert_eq!(
            providers[0].get("name").and_then(|v| v.as_str()),
            Some("kunbot-ris")
        );
        assert_eq!(
            providers[0].get("base-url").and_then(|v| v.as_str()),
            Some("https://example.com/ris/v1")
        );
        let models = providers[0]
            .get("models")
            .and_then(|value| value.as_sequence())
            .expect("models parse as a list");
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].get("name").and_then(|v| v.as_str()),
            Some("zai-org/GLM-5.3-Flash")
        );
        assert_eq!(
            models[0].get("alias").and_then(|v| v.as_str()),
            Some("glm-5.3-flash")
        );
    }

    #[test]
    fn import_providers_rewrites_the_section_and_keeps_the_rest() {
        let paths = paths();
        write_config(&paths, &cpa_local(8317), "token-a").unwrap();
        let toml = "
[[codex-api-key]]
api-key = \"sk-test\"
base-url = \"https://example.com/acid/v1\"

[[codex-api-key.models]]
name = \"gpt-5.6-terra\"
alias = \"gpt-5.6-terra\"
"
        .trim()
        .to_owned();
        import_providers(&paths, &toml).unwrap();
        let config = fs::read_to_string(config_path(&paths)).unwrap();
        assert!(config.contains("port: 8317"));
        assert!(config.contains(PROVIDERS_HEADER));
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(&config).expect("managed config stays valid YAML");
        let codex_keys = parsed
            .get("codex-api-key")
            .and_then(|value| value.as_sequence())
            .expect("codex-api-key parses as a list");
        assert_eq!(codex_keys.len(), 1);
        assert_eq!(
            codex_keys[0].get("api-key").and_then(|v| v.as_str()),
            Some("sk-test")
        );
        let models = codex_keys[0]
            .get("models")
            .and_then(|value| value.as_sequence())
            .expect("models parse as a list");
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0].get("alias").and_then(|v| v.as_str()),
            Some("gpt-5.6-terra")
        );
        assert_eq!(config.matches(PROVIDERS_HEADER).count(), 1);
        // Re-import replaces rather than appends.
        import_providers(&paths, &toml).unwrap();
        let again = fs::read_to_string(config_path(&paths)).unwrap();
        assert_eq!(again.matches("codex-api-key:").count(), 1);
    }

    #[test]
    fn plist_renders_config_path_and_escapes_paths() {
        let plist = render_agent(
            Path::new("/tmp/a&b/cli-proxy-api"),
            Path::new("/tmp/root/cpa/config.yaml"),
            Path::new("/tmp/root/logs/out.log"),
            Path::new("/tmp/root/logs/err.log"),
        );
        assert!(plist.contains("/tmp/a&amp;b/cli-proxy-api"));
        assert!(plist.contains("<string>-config</string>"));
        assert!(plist.contains("/tmp/root/cpa/config.yaml"));
        assert!(plist.contains("dev.codexmux.cpa"));
    }

    #[test]
    fn profiles_save_list_and_remove() {
        let paths = paths();
        save_profile(
            &paths,
            CpaProfile {
                name: "local".into(),
                base_url: "http://127.0.0.1:8317/v1".into(),
                token: "token-a".into(),
            },
        )
        .unwrap();
        save_profile(
            &paths,
            CpaProfile {
                name: "remote".into(),
                base_url: "https://cpa.example.com/v1".into(),
                token: "token-b".into(),
            },
        )
        .unwrap();
        let (active, saved) = profiles(&paths);
        assert_eq!(active, None);
        assert_eq!(saved.len(), 2);
        assert_eq!(saved[0].name, "local");

        // Updating an existing name replaces the entry instead of duplicating.
        save_profile(
            &paths,
            CpaProfile {
                name: "local".into(),
                base_url: "http://127.0.0.1:9317/v1".into(),
                token: "token-a2".into(),
            },
        )
        .unwrap();
        let (_, updated) = profiles(&paths);
        assert_eq!(updated.len(), 2);
        assert_eq!(updated[0].base_url, "http://127.0.0.1:9317/v1");

        remove_profile(&paths, "local").unwrap();
        let (_, remaining) = profiles(&paths);
        assert_eq!(remaining.len(), 1);
        let error = remove_profile(&paths, "local").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn profile_validation_rejects_bad_urls_before_any_switch() {
        let paths = paths();
        // Remote HTTP is rejected at save time by the endpoint shape check.
        let error = save_profile(
            &paths,
            CpaProfile {
                name: "bad".into(),
                base_url: "http://external.example.com/v1".into(),
                token: String::new(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("must use HTTPS"));
        // Nothing was switched: no active profile recorded.
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
        // An unreachable-but-valid endpoint fails switch validation.
        save_profile(
            &paths,
            CpaProfile {
                name: "unreachable".into(),
                base_url: "https://cpa-unreachable.example.com/v1".into(),
                token: String::new(),
            },
        )
        .unwrap();
        let error = switch_profile(&paths, "unreachable").unwrap_err();
        assert!(error.to_string().contains("failed validation"));
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
    }

    #[test]
    fn switch_to_unknown_profile_fails_without_changes() {
        let paths = paths();
        let error = switch_profile(&paths, "missing").unwrap_err();
        assert!(error.to_string().contains("does not exist"));
        let (active, _) = profiles(&paths);
        assert_eq!(active, None);
    }
}
