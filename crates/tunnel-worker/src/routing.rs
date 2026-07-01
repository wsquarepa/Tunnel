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
        if host != apex && host.ends_with(apex) {
            let label = host.strip_suffix(apex)?.trim_end_matches('.');
            let label = label.rsplit('.').next().unwrap_or(label);
            if label.is_empty() {
                return None;
            }
            return Some(Resolved {
                kind: "subdomain",
                matcher: label.to_string(),
                local_path: path.to_string(),
            });
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn reserved_helper() {
        assert!(is_reserved_slug("admin"));
        assert!(!is_reserved_slug("jupyter"));
    }
}
