use std::path::Path;
use std::process::Command;

use serde_json::Value;

struct Run {
    code: i32,
    report: Value,
    stderr: String,
}

fn site(dir: &Path, files: &[(&str, &str)]) {
    for (name, content) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

fn cssdiff(before: &Path, after: &Path, args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_cssdiff"))
        .args(args)
        .arg("--json")
        .arg(before)
        .arg(after)
        .output()
        .unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    Run {
        code: out.status.code().unwrap(),
        report: serde_json::from_str(&stdout).unwrap_or(Value::Null),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

/// `(element, property, before, after, viewports)` for every change.
fn changes(run: &Run) -> Vec<(String, String, String, String, Vec<String>)> {
    run.report["pages"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| p["changes"].as_array().unwrap().iter())
        .map(|c| {
            let s = |k: &str| c[k].as_str().unwrap().to_string();
            let viewports = c["viewports"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            (
                s("element"),
                s("property"),
                s("before"),
                s("after"),
                viewports,
            )
        })
        .collect()
}

fn compare(html: &str, before_css: &str, after_css: &str, args: &[&str]) -> Run {
    compare_html(html, html, before_css, after_css, args)
}

fn compare_html(
    before_html: &str,
    after_html: &str,
    before_css: &str,
    after_css: &str,
    args: &[&str],
) -> Run {
    let tmp = tempfile::tempdir().unwrap();
    let (b, a) = (tmp.path().join("before"), tmp.path().join("after"));
    site(&b, &[("index.html", before_html), ("app.css", before_css)]);
    site(&a, &[("index.html", after_html), ("app.css", after_css)]);
    cssdiff(&b, &a, args)
}

fn page(body: &str) -> String {
    format!(
        r#"<!doctype html><html><head><link rel="stylesheet" href="app.css"></head><body>{body}</body></html>"#
    )
}

#[test]
fn identical_sites_exit_zero() {
    let run = compare(
        &page("<p class=a>x</p>"),
        ".a{color:red}",
        ".a{color:red}",
        &[],
    );
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(changes(&run).is_empty());
}

#[test]
fn equivalent_rewrite_is_identical() {
    let run = compare(
        &page("<p class=a>x</p>"),
        ".a{color:red;margin:0 0 0 0;padding:1rem}",
        "p.a{color:#f00}.a{margin:0;padding:16px}",
        &[],
    );
    assert_eq!(run.code, 0, "{:?}", changes(&run));
}

#[test]
fn media_query_breakpoint_change_is_reported_per_viewport() {
    let run = compare(
        &page("<main class=w>x</main>"),
        ".w{padding:8px}@media (min-width:768px){.w{padding:24px}}",
        ".w{padding:8px}@media (min-width:1024px){.w{padding:24px}}",
        &["-p", "padding-top"],
    );
    assert_eq!(run.code, 1);
    assert_eq!(
        changes(&run),
        vec![(
            "html > body > main.w".into(),
            "padding-top".into(),
            "24px".into(),
            "8px".into(),
            vec!["768x1024".into()]
        )]
    );
}

#[test]
fn specificity_and_order_decide_the_winner() {
    let run = compare(
        &page("<p class=a id=b>x</p>"),
        ".a{color:red}.a{color:blue}",
        ".a{color:blue}.a{color:red}",
        &["-p", "color"],
    );
    let c = changes(&run);
    assert_eq!(c.len(), 1);
    assert_eq!(
        (c[0].2.as_str(), c[0].3.as_str()),
        ("rgb(0, 0, 255)", "rgb(255, 0, 0)")
    );

    let run = compare(
        &page("<p class=a id=b>x</p>"),
        "#b{color:green}.a{color:red}",
        "#b{color:green}.a{color:red !important}",
        &["-p", "color"],
    );
    let c = changes(&run);
    assert_eq!(
        (c[0].2.as_str(), c[0].3.as_str()),
        ("rgb(0, 128, 0)", "rgb(255, 0, 0)")
    );
}

#[test]
fn cascade_layer_order_is_honored() {
    let rules = "@layer base{p{color:red}}@layer theme{p{color:blue}}";
    let run = compare(
        &page("<p>x</p>"),
        &format!("@layer base, theme;{rules}"),
        &format!("@layer theme, base;{rules}"),
        &["-p", "color"],
    );
    let c = changes(&run);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(
        (c[0].2.as_str(), c[0].3.as_str()),
        ("rgb(0, 0, 255)", "rgb(255, 0, 0)")
    );
}

#[test]
fn custom_property_change_reaches_its_users() {
    let run = compare(
        &page("<p class=a>x</p>"),
        ":root{--c:#000}.a{color:var(--c)}",
        ":root{--c:#111}.a{color:var(--c)}",
        &["-p", "color"],
    );
    let elements: Vec<_> = changes(&run).into_iter().map(|c| c.0).collect();
    assert_eq!(elements, vec!["html > body > p.a"]);
}

#[test]
fn inherited_change_is_reported_on_descendants() {
    let run = compare(
        &page("<div><span>x</span></div>"),
        "body{color:red}",
        "body{color:blue}",
        &["-p", "color"],
    );
    let elements: Vec<_> = changes(&run).into_iter().map(|c| c.0).collect();
    assert_eq!(
        elements,
        vec![
            "html > body",
            "html > body > div",
            "html > body > div > span"
        ]
    );
}

#[test]
fn pseudo_elements_are_compared() {
    let run = compare(
        &page("<a class=b>x</a>"),
        ".b::before{content:'>';color:red}",
        ".b::before{content:'>';color:blue}",
        &["-p", "color"],
    );
    let elements: Vec<_> = changes(&run).into_iter().map(|c| c.0).collect();
    assert_eq!(elements, vec!["html > body > a.b::before"]);
}

#[test]
fn elements_inside_display_none_are_compared() {
    let run = compare(
        &page("<div hidden><p class=a>x</p></div>"),
        ".a{color:red}",
        ".a{color:blue}",
        &["-p", "color"],
    );
    let elements: Vec<_> = changes(&run).into_iter().map(|c| c.0).collect();
    assert_eq!(elements, vec!["html > body > div > p.a"]);
}

#[test]
fn renamed_classes_with_the_same_styles_are_identical() {
    let run = compare_html(
        &page("<p class=old-name>x</p>"),
        &page("<p class=new-name>x</p>"),
        ".old-name{color:red}",
        ".new-name{color:red}",
        &[],
    );
    assert_eq!(run.code, 0, "{:?}", changes(&run));
}

#[test]
fn markup_differences_are_reported_as_unmatched() {
    let run = compare_html(&page("<p>x</p>"), &page("<p>x</p><p>y</p>"), "", "", &[]);
    assert_eq!(run.code, 1);
    let unmatched = &run.report["pages"][0]["unmatched"][0];
    assert_eq!(unmatched["element"], "html > body > p[2]");
    assert_eq!(unmatched["path"], "/html[1]/body[1]/p[2]");
    assert_eq!(unmatched["only_in"], "after");
}

#[test]
fn used_mode_reports_layout_values() {
    let html = page("<div class=p><div class=c>x</div></div>");
    let before = ".p{width:400px}.c{width:50%}";
    let after = ".p{width:600px}.c{width:50%}";
    let computed = compare(&html, before, after, &["-p", "width", "-v", "1000x800"]);
    let elements: Vec<_> = changes(&computed)
        .into_iter()
        .map(|c| (c.0, c.2, c.3))
        .collect();
    assert_eq!(
        elements,
        vec![("html > body > div.p".into(), "400px".into(), "600px".into())]
    );

    let used = compare(
        &html,
        before,
        after,
        &["-p", "width", "-v", "1000x800", "-m", "used"],
    );
    let elements: Vec<_> = changes(&used)
        .into_iter()
        .map(|c| (c.0, c.2, c.3))
        .collect();
    assert!(
        elements.contains(&(
            "html > body > div.p > div.c".into(),
            "200px".into(),
            "300px".into()
        )),
        "{elements:?}"
    );
}

#[test]
fn base_path_maps_root_relative_urls() {
    let tmp = tempfile::tempdir().unwrap();
    let html = r#"<!doctype html><link rel="stylesheet" href="/docs/a/app.css?v=3"><p>x</p>"#;
    let (b, a) = (tmp.path().join("b"), tmp.path().join("a"));
    site(&b, &[("index.html", html), ("a/app.css", "p{color:red}")]);
    site(&a, &[("index.html", html), ("a/app.css", "p{color:blue}")]);

    let run = cssdiff(&b, &a, &["-b", "/docs", "-p", "color"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(!run.stderr.contains("warning"), "{}", run.stderr);

    let run = cssdiff(&b, &a, &["-p", "color"]);
    assert!(run.stderr.contains("cannot read"), "{}", run.stderr);
}

#[test]
fn imports_are_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let html = page("<p>x</p>");
    let (b, a) = (tmp.path().join("b"), tmp.path().join("a"));
    site(
        &b,
        &[
            ("index.html", &html),
            ("app.css", "@import 'x.css';"),
            ("x.css", "p{color:red}"),
        ],
    );
    site(
        &a,
        &[
            ("index.html", &html),
            ("app.css", "@import 'x.css';"),
            ("x.css", "p{color:blue}"),
        ],
    );
    let run = cssdiff(&b, &a, &["-p", "color"]);
    assert_eq!(changes(&run).len(), 1, "{}", run.stderr);
}

#[test]
fn pages_only_on_one_side_are_differences() {
    let tmp = tempfile::tempdir().unwrap();
    let (b, a) = (tmp.path().join("b"), tmp.path().join("a"));
    site(&b, &[("index.html", "<p>x</p>")]);
    site(
        &a,
        &[("index.html", "<p>x</p>"), ("docs/new.html", "<p>y</p>")],
    );
    let run = cssdiff(&b, &a, &[]);
    assert_eq!(run.code, 1);
    assert_eq!(run.report["only_after"][0], "docs/new.html");
}

#[test]
fn errors_exit_two() {
    let tmp = tempfile::tempdir().unwrap();
    let run = cssdiff(&tmp.path().join("nope"), tmp.path(), &[]);
    assert_eq!(run.code, 2);
    let run = cssdiff(tmp.path(), tmp.path(), &["-p", "inline-size"]);
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("logical"), "{}", run.stderr);
}

#[test]
fn grid_properties_are_compared() {
    let run = compare(
        &page("<ul class=g><li>a</li></ul>"),
        ".g{display:grid;grid-template-columns:1fr}@media (min-width:768px){.g{grid-template-columns:repeat(3,1fr)}}",
        ".g{display:grid;grid-template-columns:1fr}@media (min-width:1024px){.g{grid-template-columns:repeat(3,1fr)}}",
        &["-p", "grid-template-columns,column-count"],
    );
    let c = changes(&run);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(
        (c[0].1.as_str(), c[0].4.clone()),
        ("grid-template-columns", vec!["768x1024".to_string()])
    );
}

#[test]
fn changes_are_reported_in_document_order() {
    let run = compare(
        &page("<main class=m><p class=p>x</p></main>"),
        ".p{color:red}@media (min-width:768px){.m{padding:2px}}",
        ".p{color:blue}",
        &["-p", "color,padding-top"],
    );
    let elements: Vec<_> = changes(&run).into_iter().map(|c| c.0).collect();
    assert_eq!(
        elements,
        vec!["html > body > main.m", "html > body > main.m > p.p"]
    );
}

#[test]
fn hover_rules_are_compared_with_hover() {
    let html = page("<div class=card><a class=t>x</a></div>");
    let before = ".t{color:black}.card:hover .t{color:red}";
    let after = ".t{color:black}.card:hover .t{color:blue}";
    let run = compare(
        &html,
        before,
        after,
        &["-p", "color", "-v", "1000x800", "--hover"],
    );
    assert_eq!(run.code, 1);
    assert_eq!(
        changes(&run),
        vec![(
            "html > body > div.card > a.t".into(),
            "color".into(),
            "rgb(255, 0, 0)".into(),
            "rgb(0, 0, 255)".into(),
            vec!["1000x800:hover".into()]
        )]
    );

    let default = compare(&html, before, after, &["-p", "color"]);
    assert_eq!(default.code, 0, "{:?}", changes(&default));
}

#[test]
fn focus_and_active_rules_are_compared_with_their_flags() {
    let html = page("<button class=b>x</button>");
    let before = ".b:focus{outline-style:solid}.b:active{opacity:.5}";
    let after = ".b:focus{outline-style:dashed}.b:active{opacity:.7}";
    let args = ["-p", "outline-style,opacity", "-v", "1000x800"];

    let focus = compare(&html, before, after, &[&args[..], &["--focus"]].concat());
    assert_eq!(
        changes(&focus),
        vec![(
            "html > body > button.b".into(),
            "outline-style".into(),
            "solid".into(),
            "dashed".into(),
            vec!["1000x800:focus".into()]
        )]
    );

    let active = compare(&html, before, after, &[&args[..], &["--active"]].concat());
    assert_eq!(
        changes(&active),
        vec![(
            "html > body > button.b".into(),
            "opacity".into(),
            "0.5".into(),
            "0.7".into(),
            vec!["1000x800:active".into()]
        )]
    );
}

#[test]
fn changes_without_a_state_are_not_repeated_under_one() {
    let run = compare(
        &page("<p class=a>x</p>"),
        ".a{color:red}",
        ".a{color:blue}",
        &[
            "-p", "color", "-v", "1000x800", "--hover", "--focus", "--active",
        ],
    );
    assert_eq!(
        changes(&run),
        vec![(
            "html > body > p.a".into(),
            "color".into(),
            "rgb(255, 0, 0)".into(),
            "rgb(0, 0, 255)".into(),
            vec!["1000x800".into()]
        )]
    );
}

/// Serves `root` over HTTP until the test process exits: `/x` maps to
/// `x`, `x.html` or `x/index.html`; anything else is a 404.
fn serve(root: &Path) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() == 0 || header == "\r\n" {
                    break;
                }
            }
            let path = line
                .split(' ')
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/');
            let found = [
                root.join(path),
                root.join(format!("{path}.html")),
                root.join(path).join("index.html"),
            ]
            .into_iter()
            .find(|p| p.is_file());
            let (status, body) = match found {
                Some(p) => ("200 OK", std::fs::read(p).unwrap()),
                None => ("404 Not Found", b"<!doctype html><p>missing</p>".to_vec()),
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    format!("http://{addr}")
}

fn cssdiff_urls(before: &str, after: &str, args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_cssdiff"))
        .args(args)
        .arg("--json")
        .arg(before)
        .arg(after)
        .output()
        .unwrap();
    Run {
        code: out.status.code().unwrap(),
        report: serde_json::from_str(&String::from_utf8(out.stdout).unwrap())
            .unwrap_or(Value::Null),
        stderr: String::from_utf8(out.stderr).unwrap(),
    }
}

fn two_servers(
    before: &[(&str, &str)],
    after: &[(&str, &str)],
) -> (tempfile::TempDir, String, String) {
    let tmp = tempfile::tempdir().unwrap();
    let (b, a) = (tmp.path().join("before"), tmp.path().join("after"));
    site(&b, before);
    site(&a, after);
    let (bu, au) = (serve(&b), serve(&a));
    (tmp, bu, au)
}

#[test]
fn servers_are_compared_route_by_route() {
    let html = r#"<!doctype html><html><head><link rel="stylesheet" href="/assets/app.css"></head><body><p class=a>x</p></body></html>"#;
    let (_tmp, b, a) = two_servers(
        &[
            ("index.html", html),
            ("docs/index.html", html),
            ("assets/app.css", ".a{color:red}"),
        ],
        &[
            ("index.html", html),
            ("docs/index.html", html),
            ("assets/app.css", ".a{color:blue}"),
        ],
    );
    let run = cssdiff_urls(
        &b,
        &a,
        &["-p", "color", "-v", "1000x800", "--routes", "/,/docs"],
    );
    assert_eq!(run.code, 1, "{}", run.stderr);
    let pages: Vec<_> = run.report["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["page"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(pages, vec!["/", "/docs"]);
    assert_eq!(changes(&run).len(), 2);
    assert!(
        run.stderr.is_empty() || !run.stderr.contains("warning"),
        "{}",
        run.stderr
    );

    let same = cssdiff_urls(&b, &b, &["--routes", "/,/docs"]);
    assert_eq!(same.code, 0, "{}", same.stderr);
}

#[test]
fn http_status_changes_are_differences() {
    let html = "<!doctype html><p>x</p>";
    let (_tmp, b, a) = two_servers(&[("gone.html", html)], &[("index.html", html)]);
    let run = cssdiff_urls(&b, &a, &["--routes", "/gone"]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(
        run.report["pages"][0]["status"],
        serde_json::json!([200, 404])
    );
}

#[test]
fn routes_files_are_read() {
    let html = "<!doctype html><p>x</p>";
    let (tmp, b, a) = two_servers(
        &[("index.html", html), ("x.html", html)],
        &[("index.html", html), ("x.html", html)],
    );
    let list = tmp.path().join("routes.txt");
    std::fs::write(&list, "# pages\n/\n\n/x  # second\n").unwrap();
    let run = cssdiff_urls(&b, &a, &["--routes", list.to_str().unwrap()]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(run.report["pages"].as_array().unwrap().len(), 2);
}

#[test]
fn servers_need_routes_and_do_not_mix_with_files() {
    let (_tmp, b, a) = two_servers(&[("index.html", "<p>x</p>")], &[("index.html", "<p>x</p>")]);
    let no_routes = cssdiff_urls(&b, &a, &[]);
    assert_eq!(no_routes.code, 2);
    assert!(
        no_routes.stderr.contains("--routes"),
        "{}",
        no_routes.stderr
    );

    let mixed = cssdiff_urls(&b, "/tmp", &["--routes", "/"]);
    assert_eq!(mixed.code, 2);

    let unreachable = cssdiff_urls("http://127.0.0.1:9", &a, &["--routes", "/"]);
    assert_eq!(unreachable.code, 2, "{}", unreachable.stderr);
}

#[test]
fn urls_are_compared_without_the_server_origin() {
    let html = r#"<!doctype html><html><head><link rel="stylesheet" href="/assets/app.css"></head><body><p class=a>x</p><p class=b>y</p></body></html>"#;
    let css = ".a{background-image:url(/assets/a.svg)}.b{mask-image:url(b.svg)}";
    let (_tmp, b, a) = two_servers(
        &[("index.html", html), ("assets/app.css", css)],
        &[
            ("index.html", html),
            ("assets/app.css", &css.replace("b.svg", "c.svg")),
        ],
    );
    let props = ["-p", "background-image,mask-image", "-v", "1000x800"];
    let same = cssdiff_urls(
        &b,
        &b.replace("127.0.0.1", "localhost"),
        &[&props[..], &["--routes", "/"]].concat(),
    );
    assert_eq!(same.code, 0, "{}", same.report);

    let run = cssdiff_urls(&b, &a, &[&props[..], &["--routes", "/"]].concat());
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert_eq!(
        changes(&run)
            .into_iter()
            .map(|(_, p, before, after, _)| (p, before, after))
            .collect::<Vec<_>>(),
        vec![(
            "mask-image".to_string(),
            r#"url("/assets/b.svg")"#.to_string(),
            r#"url("/assets/c.svg")"#.to_string(),
        )]
    );
}
