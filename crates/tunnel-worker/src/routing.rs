pub const RESERVED_SLUGS: [&str; 2] = ["admin", "_tunnel"];

pub fn is_reserved_slug(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub kind: &'static str,
    pub matcher: String,
    pub local_path: String,
}

/// Resolve a public (host, path) to a route matcher and the local path to send upstream.
pub fn resolve(host: &str, path: &str, apex_host: Option<&str>) -> Option<Resolved> {
    if let Some(apex) = apex_host {
        if host != apex {
            if let Some(prefix) = host.strip_suffix(apex) {
                if let Some(label) = prefix.strip_suffix('.') {
                    // the label immediately left of the apex is the tenant
                    let label = label.rsplit('.').next().unwrap_or(label);
                    if !label.is_empty() {
                        return Some(Resolved {
                            kind: "subdomain",
                            matcher: label.to_string(),
                            local_path: path.to_string(),
                        });
                    }
                }
            }
        }
    }

    let trimmed = path.trim_start_matches('/');
    let slug = trimmed.split('/').next().unwrap_or("");
    if slug.is_empty() || is_reserved_slug(slug) {
        return None;
    }
    let rest = &trimmed[slug.len()..]; // begins with '/' or is empty
    let local_path = if rest.is_empty() {
        "/".to_string()
    } else {
        rest.to_string()
    };
    Some(Resolved {
        kind: "path",
        matcher: slug.to_string(),
        local_path,
    })
}

/// Re-attach the public request's raw query string to the local path so the
/// upstream sees `path?query` exactly as the caller sent it. `ReqHead.path` and
/// `WsOpen.path` carry the query inline; there is no separate field.
pub fn with_query(local_path: &str, query: Option<&str>) -> String {
    match query {
        Some(q) => format!("{local_path}?{q}"),
        None => local_path.to_string(),
    }
}

/// Redirect target for a path-mode bare slug (`/gradio`, no trailing slash).
///
/// Browsers treat that URL as a file under `/`, so relative asset URLs from
/// upstream SPAs (`<base href="./">`, `./assets/...`) resolve against the domain
/// root and the page blanks. Derives the condition from what `resolve` already
/// computed (local path `/` with an unslashed public path), so doubled leading
/// slashes and reserved slugs follow the same rules as routing itself. Returns
/// `/{slug}/` with the query re-attached, or `None` when the public path is
/// already slashed, has a subpath, or the route is subdomain mode.
pub fn bare_slug_redirect(
    resolved: &Resolved,
    public_path: &str,
    query: Option<&str>,
) -> Option<String> {
    if resolved.kind != "path" || resolved.local_path != "/" || public_path.ends_with('/') {
        return None;
    }
    Some(with_query(&format!("/{}/", resolved.matcher), query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_query_appends_raw_query() {
        assert_eq!(with_query("/api", Some("x=1&y=%20z")), "/api?x=1&y=%20z");
        assert_eq!(with_query("/", Some("x=1")), "/?x=1");
    }

    #[test]
    fn with_query_without_query_is_identity() {
        assert_eq!(with_query("/api", None), "/api");
    }

    #[test]
    fn path_mode_strips_prefix() {
        let r = resolve("tunnel.workers.dev", "/jupyter/lab/tree", None).unwrap();
        assert_eq!(r.kind, "path");
        assert_eq!(r.matcher, "jupyter");
        assert_eq!(r.local_path, "/lab/tree");
    }

    #[test]
    fn path_mode_bare_slug_maps_to_root() {
        let r = resolve("tunnel.workers.dev", "/jupyter", None).unwrap();
        assert_eq!(r.local_path, "/");
    }

    #[test]
    fn path_mode_rejects_reserved() {
        assert!(resolve("tunnel.workers.dev", "/admin/login", None).is_none());
        assert!(resolve("tunnel.workers.dev", "/_tunnel/connect", None).is_none());
    }

    #[test]
    fn path_mode_rejects_empty() {
        assert!(resolve("tunnel.workers.dev", "/", None).is_none());
    }

    #[test]
    fn subdomain_mode_uses_label_and_keeps_path() {
        let r = resolve(
            "jupyter.tunnel.example.com",
            "/lab/tree",
            Some("tunnel.example.com"),
        )
        .unwrap();
        assert_eq!(r.kind, "subdomain");
        assert_eq!(r.matcher, "jupyter");
        assert_eq!(r.local_path, "/lab/tree");
    }

    #[test]
    fn apex_host_itself_falls_back_to_path_mode() {
        // Hitting the apex directly is not a subdomain match.
        let r = resolve(
            "tunnel.example.com",
            "/ollama/api",
            Some("tunnel.example.com"),
        )
        .unwrap();
        assert_eq!(r.kind, "path");
        assert_eq!(r.matcher, "ollama");
    }

    #[test]
    fn ends_with_apex_but_not_subdomain_is_path_mode() {
        // apex suffix without a dot boundary must NOT be a subdomain match
        let r = resolve(
            "nottunnel.example.com",
            "/foo/bar",
            Some("tunnel.example.com"),
        )
        .unwrap();
        assert_eq!(r.kind, "path");
        assert_eq!(r.matcher, "foo");
        assert_eq!(r.local_path, "/bar");
    }

    #[test]
    fn reserved_helper() {
        assert!(is_reserved_slug("admin"));
        assert!(!is_reserved_slug("jupyter"));
    }

    fn redirect_for(path: &str, query: Option<&str>) -> Option<String> {
        let resolved = resolve("tunnel.workers.dev", path, None).unwrap();
        bare_slug_redirect(&resolved, path, query)
    }

    #[test]
    fn bare_slug_redirects_to_slash_form() {
        assert_eq!(redirect_for("/gradio", None).as_deref(), Some("/gradio/"));
        assert_eq!(
            redirect_for("/gradio", Some("x=1")).as_deref(),
            Some("/gradio/?x=1")
        );
        assert_eq!(redirect_for("//gradio", None).as_deref(), Some("/gradio/"));
    }

    #[test]
    fn bare_slug_redirect_skips_already_slashed_and_subpaths() {
        assert_eq!(redirect_for("/gradio/", None), None);
        assert_eq!(redirect_for("/gradio/config", None), None);
    }

    #[test]
    fn bare_slug_redirect_skips_subdomain_mode() {
        let resolved =
            resolve("gradio.tunnel.example.com", "/", Some("tunnel.example.com")).unwrap();
        assert_eq!(bare_slug_redirect(&resolved, "/", None), None);
    }
}
