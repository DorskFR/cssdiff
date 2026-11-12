use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Result, bail};
use clap::Parser;

use cssdiff::diff::{Sources, diff_page};
use cssdiff::page::Source;
use cssdiff::report::{PageReport, Report};
use cssdiff::{Mode, Size, State, property, site};

/// Compare the computed styles of every element of two versions of a page or
/// site, at every viewport, without a browser.
///
/// Exits 0 when identical, 1 when computed styles differ, 2 on error.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// HTML file, directory or `http://` server of the reference version
    #[arg(required_unless_present = "list_properties")]
    before: Option<String>,
    /// HTML file, directory or `http://` server of the changed version
    #[arg(required_unless_present = "list_properties")]
    after: Option<String>,
    /// Routes to fetch from both servers: `/path` values, or files listing one
    /// route per line (repeatable or comma separated)
    #[arg(short, long, value_delimiter = ',')]
    routes: Vec<String>,
    /// Seconds allowed per HTTP request
    #[arg(long, default_value_t = 30)]
    timeout: u64,
    /// Print the compared properties, one per line, and exit
    #[arg(long)]
    list_properties: bool,
    /// Viewport sizes, WIDTHxHEIGHT (repeatable or comma separated)
    #[arg(
        short,
        long = "viewport",
        value_delimiter = ',',
        default_value = "375x812,768x1024,1440x900"
    )]
    viewports: Vec<Size>,
    /// Only compare these properties (repeatable or comma separated)
    #[arg(short, long = "property", value_delimiter = ',')]
    properties: Vec<String>,
    /// Skip these properties (repeatable or comma separated)
    #[arg(short, long = "ignore", value_delimiter = ',')]
    ignore: Vec<String>,
    /// URL path prefix the site is served under (e.g. `/docs` when pages link
    /// `/docs/assets/app.css`)
    #[arg(short, long, default_value = "")]
    base: String,
    /// Which values to compare
    #[arg(short, long, value_enum, default_value_t = Mode::Computed)]
    mode: Mode,
    /// Also compare with `:hover` forced on every element
    #[arg(long)]
    hover: bool,
    /// Also compare with `:focus` forced on every element
    #[arg(long)]
    focus: bool,
    /// Also compare with `:active` forced on every element
    #[arg(long)]
    active: bool,
    /// Print the report as JSON
    #[arg(long)]
    json: bool,
    /// Pages processed in parallel (default: number of CPUs)
    #[arg(short, long)]
    jobs: Option<usize>,
}

/// `Some(origin)` when `arg` is a server URL.
fn server(arg: &str) -> Result<Option<url::Url>> {
    if arg.starts_with("https://") {
        bail!("https is not supported: point cssdiff at the server's http:// address");
    }
    if !arg.starts_with("http://") {
        return Ok(None);
    }
    let url = url::Url::parse(arg).map_err(|e| anyhow::anyhow!("invalid URL {arg}: {e}"))?;
    Ok(Some(url))
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(true) => ExitCode::from(1),
        Ok(false) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cssdiff: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
    let start = Instant::now();
    let known = property::all();
    if cli.list_properties {
        for p in &known {
            println!("{}", p.name);
        }
        return Ok(false);
    }
    let (Some(before), Some(after)) = (&cli.before, &cli.after) else {
        unreachable!("clap requires both paths");
    };
    for p in cli.properties.iter().chain(&cli.ignore) {
        if !known.iter().any(|k| k.name == p) {
            bail!("unknown or logical CSS longhand {p:?}");
        }
    }
    let properties: Vec<_> = known
        .into_iter()
        .filter(|p| cli.properties.is_empty() || cli.properties.iter().any(|q| q == p.name))
        .filter(|p| !cli.ignore.iter().any(|q| q == p.name))
        .collect();

    let states: Vec<State> = [
        (cli.hover, State::Hover),
        (cli.focus, State::Focus),
        (cli.active, State::Active),
    ]
    .into_iter()
    .filter_map(|(on, state)| on.then_some(state))
    .collect();

    let (plan, before, after) = match (server(before)?, server(after)?) {
        (Some(b), Some(a)) => {
            let routes = site::routes(&cli.routes)?;
            if routes.is_empty() {
                bail!("comparing servers needs --routes");
            }
            let agent = cssdiff::net::agent(std::time::Duration::from_secs(cli.timeout));
            (
                site::plan_routes(&routes),
                Source::Http {
                    origin: b,
                    agent: agent.clone(),
                },
                Source::Http { origin: a, agent },
            )
        }
        (None, None) => {
            if !cli.routes.is_empty() {
                bail!("--routes only applies to http:// servers");
            }
            let plan = site::plan(Path::new(before), Path::new(after))?;
            let dir = |root: &PathBuf| Source::Dir {
                root: root.clone(),
                base: cli.base.clone(),
            };
            let (b, a) = (dir(&plan.before_root), dir(&plan.after_root));
            (plan, b, a)
        }
        _ => bail!("compare two servers, or two local files or directories"),
    };
    let sources = Sources {
        before: &before,
        after: &after,
    };
    let jobs = cli
        .jobs
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| n.get()))
        .clamp(1, plan.pairs.len().max(1));

    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Result<PageReport>>>> =
        Mutex::new((0..plan.pairs.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((b, a)) = plan.pairs.get(i) else {
                        break;
                    };
                    let name = if b == a {
                        b.display().to_string()
                    } else {
                        format!("{} → {}", b.display(), a.display())
                    };
                    let result = diff_page(
                        &sources,
                        b,
                        a,
                        &cli.viewports,
                        &properties,
                        cli.mode,
                        &states,
                    )
                    .map(|d| Report::page(name, d));
                    results.lock().unwrap()[i] = Some(result);
                }
            });
        }
    });

    let mut pages = Vec::new();
    for r in results.into_inner().unwrap() {
        pages.push(r.expect("every page is processed")?);
    }
    let report = Report {
        viewports: cli.viewports.iter().map(Size::to_string).collect(),
        pages,
        only_before: plan
            .only_before
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        only_after: plan
            .only_after
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    };

    for page in &report.pages {
        for w in &page.warnings {
            eprintln!("warning: {}: {w}", page.page);
        }
    }
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.to_text());
    }

    let elements: usize = report.pages.iter().map(|p| p.elements).sum();
    let changed: usize = report.pages.iter().map(|p| p.changes.len()).sum();
    eprintln!(
        "{} page(s) × {} viewport(s) × {} properties, {} elements: {} in {:.0?}",
        report.pages.len(),
        report.viewports.len(),
        properties.len(),
        elements,
        if report.has_differences() {
            format!("{changed} computed-style difference(s)")
        } else {
            "identical".to_string()
        },
        start.elapsed()
    );
    Ok(report.has_differences())
}
