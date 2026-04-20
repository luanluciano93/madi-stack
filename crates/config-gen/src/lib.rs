#![forbid(unsafe_code)]
//! Renders nginx.conf / php.ini / my.ini from Tera templates.
//!
//! Templates live in `/templates/*.tera` at the repo root and are embedded
//! into the binary at compile time via `include_str!`. There is no runtime
//! template discovery: the crate is self-contained and hermetic.

use std::fs;
use std::path::{Path, PathBuf};

use madi_core::PortConfig;
use tera::{Context, Tera};

const TPL_NGINX: &str = include_str!("../../../templates/nginx.conf.tera");
const TPL_PHP: &str = include_str!("../../../templates/php.ini.tera");
const TPL_MY: &str = include_str!("../../../templates/my.ini.tera");
const TPL_SITE: &str = include_str!("../../../templates/site-default.conf.tera");

const NAME_NGINX: &str = "nginx.conf";
const NAME_PHP: &str = "php.ini";
const NAME_MY: &str = "my.ini";
const NAME_SITE: &str = "site-default.conf";

#[derive(Debug, thiserror::Error)]
pub enum ConfigGenError {
    #[error("template error: {0}")]
    Tera(#[from] tera::Error),

    #[error("I/O error writing {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type ConfigGenResult<T> = Result<T, ConfigGenError>;

/// Default PHP extensions enabled on first `init`.
///
/// Kept conservative on purpose: every entry here is something a PHP app
/// commonly fails without. Optional extras (`sqlsrv`, `xdebug`, `redis`,
/// `imagick`, …) are opt-in via the GUI and appended to `php_extensions`
/// at render time.
pub const DEFAULT_PHP_EXTENSIONS: &[&str] = &[
    "mbstring",
    "mysqli",
    "pdo_mysql",
    "openssl",
    "curl",
    "gd",
    "fileinfo",
    "zip",
    "intl",
    "sodium",
];

/// Inputs for every template render.
///
/// Paths are injected into configs as strings with forward slashes. Nginx
/// tokenises backslashes inside quoted strings as escapes, so Windows-style
/// `C:\foo\bar` must be normalised before it reaches the template.
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    pub install_dir: &'a Path,
    pub ports: PortConfig,
    pub document_root: &'a Path,
    /// Names of PHP extensions to enable in `php.ini` (order preserved).
    /// Use [`DEFAULT_PHP_EXTENSIONS`] for the baseline set.
    pub php_extensions: &'a [&'a str],
}

impl RenderContext<'_> {
    fn to_tera(&self) -> Context {
        let mut ctx = Context::new();
        ctx.insert("install_dir", &path_to_posix(self.install_dir));
        ctx.insert("document_root", &path_to_posix(self.document_root));
        ctx.insert("ports", &self.ports);
        ctx.insert("php_extensions", self.php_extensions);
        ctx
    }
}

/// Render `nginx.conf`, `php.ini`, and `my.ini` into `config_dir`.
///
/// `config_dir` is created (including parents) if it does not exist. Existing
/// files are overwritten — the caller is responsible for any backup policy.
pub fn render_all(ctx: &RenderContext<'_>, config_dir: &Path) -> ConfigGenResult<()> {
    fs::create_dir_all(config_dir).map_err(|source| ConfigGenError::Io {
        path: config_dir.to_path_buf(),
        source,
    })?;

    let tera = build_engine()?;
    let tctx = ctx.to_tera();

    for (name, out_name) in [
        (NAME_NGINX, NAME_NGINX),
        (NAME_PHP, NAME_PHP),
        (NAME_MY, NAME_MY),
    ] {
        let rendered = tera.render(name, &tctx)?;
        let out = config_dir.join(out_name);
        write_lf(&out, &rendered)?;
    }

    Ok(())
}

/// Render a virtual-host file for `site_name` and write it to `out_path`.
///
/// Used by the sprint-3 "sites" feature. `out_path` typically lives under
/// `<install>/config/sites-enabled/<site_name>.conf`.
pub fn render_site(
    ctx: &RenderContext<'_>,
    site_name: &str,
    out_path: &Path,
) -> ConfigGenResult<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigGenError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let tera = build_engine()?;
    let mut tctx = ctx.to_tera();
    tctx.insert("site_name", site_name);
    let rendered = tera.render(NAME_SITE, &tctx)?;
    write_lf(out_path, &rendered)
}

fn build_engine() -> ConfigGenResult<Tera> {
    let mut tera = Tera::default();
    // autoescape is an HTML concern; configs are plain text — disable globally.
    tera.autoescape_on(vec![]);
    tera.add_raw_templates(vec![
        (NAME_NGINX, TPL_NGINX),
        (NAME_PHP, TPL_PHP),
        (NAME_MY, TPL_MY),
        (NAME_SITE, TPL_SITE),
    ])?;
    Ok(tera)
}

fn write_lf(path: &Path, content: &str) -> ConfigGenResult<()> {
    // Normalise CRLF → LF. Tera preserves input EOLs; git may have converted
    // the embedded templates on checkout depending on .gitattributes.
    let normalised = if content.contains('\r') {
        content.replace("\r\n", "\n")
    } else {
        content.to_owned()
    };
    fs::write(path, normalised).map_err(|source| ConfigGenError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn path_to_posix(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx<'a>(install: &'a Path, docroot: &'a Path) -> RenderContext<'a> {
        RenderContext {
            install_dir: install,
            ports: PortConfig {
                http: 8080,
                mariadb: 3310,
                php_fcgi: 9001,
                bind_address: std::net::Ipv4Addr::LOCALHOST,
            },
            document_root: docroot,
            php_extensions: DEFAULT_PHP_EXTENSIONS,
        }
    }

    #[test]
    fn path_to_posix_converts_backslashes() {
        let p = PathBuf::from(r"C:\Users\João\MadiStack");
        assert_eq!(path_to_posix(&p), "C:/Users/João/MadiStack");
    }

    #[test]
    fn nginx_render_substitutes_ports_and_paths() {
        let install = PathBuf::from(r"C:\madi");
        let docroot = PathBuf::from(r"C:\madi\www");
        let c = ctx(&install, &docroot);
        let tera = build_engine().unwrap();
        let out = tera.render(NAME_NGINX, &c.to_tera()).unwrap();

        assert!(out.contains("listen       127.0.0.1:8080;"), "{out}");
        assert!(out.contains("fastcgi_pass 127.0.0.1:9001;"), "{out}");
        assert!(out.contains(r#"root "C:/madi/www";"#), "{out}");
        // Path-typed values must not carry Windows backslashes into nginx.
        assert!(!out.contains(r"C:\madi"), "raw install path leaked: {out}");
    }

    #[test]
    fn mariadb_render_uses_mariadb_port() {
        let install = PathBuf::from("/opt/madi");
        let docroot = PathBuf::from("/opt/madi/www");
        let c = ctx(&install, &docroot);
        let tera = build_engine().unwrap();
        let out = tera.render(NAME_MY, &c.to_tera()).unwrap();
        assert!(out.contains("port        = 3310"), "{out}");
        assert!(out.contains(r#"datadir     = "/opt/madi/data/mariadb""#));
    }

    #[test]
    fn php_render_embeds_install_dir() {
        let install = PathBuf::from(r"D:\stacks\madi");
        let docroot = PathBuf::from(r"D:\stacks\madi\www");
        let c = ctx(&install, &docroot);
        let tera = build_engine().unwrap();
        let out = tera.render(NAME_PHP, &c.to_tera()).unwrap();
        assert!(out.contains(r#"error_log = "D:/stacks/madi/logs/php/php-error.log""#));
        assert!(out.contains(r#"extension_dir = "D:/stacks/madi/bin/php/ext""#));
    }

    #[test]
    fn site_render_injects_site_name() {
        let install = PathBuf::from("/m");
        let docroot = PathBuf::from("/m/sites/blog");
        let c = ctx(&install, &docroot);
        let tera = build_engine().unwrap();
        let mut tctx = c.to_tera();
        tctx.insert("site_name", "blog");
        let out = tera.render(NAME_SITE, &tctx).unwrap();
        assert!(out.contains("server_name blog.test;"), "{out}");
        assert!(out.contains(r#"root "/m/sites/blog";"#));
    }

    #[test]
    fn bind_address_propagates_to_nginx_and_mariadb() {
        let install = PathBuf::from("/m");
        let docroot = PathBuf::from("/m/www");
        let mut c = ctx(&install, &docroot);
        c.ports.bind_address = std::net::Ipv4Addr::UNSPECIFIED; // 0.0.0.0

        let tera = build_engine().unwrap();
        let nginx = tera.render(NAME_NGINX, &c.to_tera()).unwrap();
        let my = tera.render(NAME_MY, &c.to_tera()).unwrap();

        assert!(nginx.contains("listen       0.0.0.0:8080;"), "{nginx}");
        assert!(my.contains("bind-address = 0.0.0.0"), "{my}");
        // FastCGI must stay local regardless of bind_address.
        assert!(nginx.contains("fastcgi_pass 127.0.0.1:9001;"), "{nginx}");
    }

    #[test]
    fn php_extensions_are_dynamic() {
        let install = PathBuf::from("/m");
        let docroot = PathBuf::from("/m/www");
        let mut c = ctx(&install, &docroot);
        let custom = ["mbstring", "sqlsrv", "pdo_sqlsrv"];
        c.php_extensions = &custom;

        let tera = build_engine().unwrap();
        let out = tera.render(NAME_PHP, &c.to_tera()).unwrap();

        assert!(out.contains("extension=sqlsrv"), "{out}");
        assert!(out.contains("extension=pdo_sqlsrv"), "{out}");
        assert!(
            !out.contains("extension=mysqli"),
            "default set leaked: {out}"
        );
    }

    #[test]
    fn render_all_writes_three_files() {
        let tmp = std::env::temp_dir().join(format!("madi-cfg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let install = PathBuf::from("/srv/madi");
        let docroot = PathBuf::from("/srv/madi/www");
        let c = ctx(&install, &docroot);

        render_all(&c, &tmp).unwrap();

        for name in [NAME_NGINX, NAME_PHP, NAME_MY] {
            let p = tmp.join(name);
            assert!(p.exists(), "missing {name}");
            let bytes = fs::read(&p).unwrap();
            assert!(!bytes.contains(&b'\r'), "{name} should be LF-only");
        }

        fs::remove_dir_all(&tmp).ok();
    }
}
