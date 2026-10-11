use lyracore_cli::cmd::{self, Command};
use lyracore_cli::error::{EXIT_FAILURE, EXIT_OK};
use lyracore_cli::http::LoopbackHttpClient;
use lyracore_cli::proc::{RealProcessInspector, RealProcessRunner};
use lyracore_cli::project::ProjectLayout;
use lyracore_cli::Result;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `dev up` is the one command whose failures are usually a missing prerequisite rather than a
    // bug — so on failure only (never on success, and never for any other command) it gets one
    // extra line pointing at `doctor`. Parsed here, ahead of `run`, so a bad-flags usage error
    // (which never reaches `dev up`'s own logic) doesn't earn the hint.
    let is_dev_up = matches!(Command::parse(&args), Ok(Command::DevUp { .. }));
    std::process::exit(match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("lyracore: {e}");
            if is_dev_up {
                eprintln!(
                    "lyracore: if this looks like a missing prerequisite, run `lyracore doctor`"
                );
            }
            e.exit_code()
        }
    });
}

fn run(args: &[String]) -> Result<i32> {
    let command = Command::parse(args)?;

    // A few commands still need a POSIX shell or Linux. Refuse them here, at the one place a user
    // reaches them, with a sentence that says so — rather than a `program not found` from a
    // subprocess three layers down. The library functions keep their own tests.
    #[cfg(windows)]
    if let Some(feature) = unsupported_on_windows(&command) {
        return Err(lyracore_cli::Error::Process(format!(
            "`{feature}` needs a POSIX shell or Linux and is not available on Windows yet. The \
             fixture realm (`lyracore dev up`) is fully supported — see \
             docs/quickstart-windows.md."
        )));
    }

    let runner = RealProcessRunner;
    let inspector = RealProcessInspector;
    // Only ever pointed at the loopback node: minting an identity and claiming the operator as it.
    let http = LoopbackHttpClient;

    match command {
        Command::Help => {
            println!("{}", cmd::USAGE);
            Ok(EXIT_OK)
        }
        Command::HelpAll => {
            println!("{}", cmd::USAGE_ALL);
            Ok(EXIT_OK)
        }
        // `doctor` reports a broken layout as a check rather than failing before it can print —
        // diagnosing "am I in the right place?" is exactly what it is for.
        Command::Doctor => {
            let checks = cmd::doctor::run(&ProjectLayout::discover());
            let blocking = cmd::doctor::report(&checks);
            Ok(if blocking { EXIT_FAILURE } else { EXIT_OK })
        }
        Command::Preflight => {
            cmd::preflight::run(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK)
        }
        Command::Publish {
            databases,
            skip_preflight,
        } => cmd::publish::run(
            &ProjectLayout::discover()?,
            &runner,
            &databases,
            skip_preflight,
        )
        .map(|_| EXIT_OK),
        Command::DevUp { bind, topology } => {
            let mut dev = cmd::dev::DevManager::new(ProjectLayout::discover()?)?;
            dev.up(&runner, &inspector, &http, bind, topology)
                .map(|_| EXIT_OK)
        }
        Command::DevStatus => {
            let dev = cmd::dev::DevManager::new(ProjectLayout::discover()?)?;
            dev.status(&runner, &inspector).map(|_| EXIT_OK)
        }
        Command::DevSmoke => {
            let dev = cmd::dev::DevManager::new(ProjectLayout::discover()?)?;
            dev.smoke(&runner, &inspector).map(|_| EXIT_OK)
        }
        Command::DevLogs(component) => {
            let dev = cmd::dev::DevManager::new(ProjectLayout::discover()?)?;
            dev.logs(component).map(|_| EXIT_OK)
        }
        Command::DevDown { forget } => {
            let mut dev = cmd::dev::DevManager::new(ProjectLayout::discover()?)?;
            dev.down(&runner, &inspector, forget).map(|_| EXIT_OK)
        }
        Command::ImportWorld(options) => cmd::import::run_world(
            &ProjectLayout::discover()?,
            &runner,
            &cmd::import::TtyPrompt::import_world(),
            &options,
        )
        .map(|_| EXIT_OK),
        Command::ImportVmaps { options } => cmd::import::run_vmaps(
            &ProjectLayout::discover()?,
            &runner,
            &cmd::import::TtyPrompt::import_vmaps(),
            &options,
        )
        .map(|_| EXIT_OK),
        Command::AccountCreate { user, source } => {
            let project = ProjectLayout::discover()?;
            cmd::account::create(&project, &user, source, &runner).map(|_| EXIT_OK)
        }
        Command::ConfigShow => cmd::config::show(&ProjectLayout::discover()?).map(|_| EXIT_OK),
        Command::ConfigSetClientData { path } => {
            cmd::config::set_client_data(&ProjectLayout::discover()?, &path).map(|_| EXIT_OK)
        }
        Command::ClientSync => {
            cmd::client::sync(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK)
        }
        Command::ClientPack { out, zip } => {
            cmd::client::pack(&ProjectLayout::discover()?, &runner, &out, zip).map(|_| EXIT_OK)
        }
        Command::PackagesAdd { source, yes } => cmd::packages::add(
            &ProjectLayout::discover()?,
            &runner,
            &cmd::import::TtyPrompt::packages_add(),
            &source,
            yes,
        )
        .map(|_| EXIT_OK),
        Command::PackagesUpdate { name, yes } => cmd::packages::git::update(
            &ProjectLayout::discover()?,
            &runner,
            &cmd::import::TtyPrompt::packages_update(),
            name.as_deref(),
            yes,
        )
        .map(|_| EXIT_OK),
        Command::PackagesList => cmd::packages::list(&ProjectLayout::discover()?).map(|_| EXIT_OK),
        Command::PackagesNew { name, from } => {
            cmd::packages::new(&ProjectLayout::discover()?, &runner, &name, &from).map(|_| EXIT_OK)
        }
        Command::PackagesBuild => {
            cmd::packages::build::run(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK)
        }
        Command::PackagesCheck => {
            cmd::packages::check::run(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK)
        }
        // The lifecycle verbs take no process runner: they move or delete a directory and print
        // what is left to run, so there is nothing for them to execute.
        Command::PackagesEnable { name } => {
            cmd::packages::lifecycle::enable(&ProjectLayout::discover()?, &name).map(|_| EXIT_OK)
        }
        Command::PackagesDisable { name, yes } => cmd::packages::lifecycle::disable(
            &ProjectLayout::discover()?,
            &cmd::packages::lifecycle::Realm {
                runner: &runner,
                http: &http,
                prompt: &cmd::import::TtyPrompt::packages_disable(),
            },
            &name,
            yes,
        )
        .map(|_| EXIT_OK),
        Command::PackagesApply(options) => cmd::packages::apply::run(
            &ProjectLayout::discover()?,
            &runner,
            &cmd::import::TtyPrompt::packages_apply(),
            &options,
        )
        .map(|_| EXIT_OK),
        Command::PackagesConfig(options) => {
            cmd::packages::config::run(&ProjectLayout::discover()?, &runner, &http, &options)
                .map(|_| EXIT_OK)
        }
        Command::PackagesRemove { name, yes } => cmd::packages::lifecycle::remove(
            &ProjectLayout::discover()?,
            &cmd::import::TtyPrompt::packages_remove(),
            &name,
            yes,
        )
        .map(|_| EXIT_OK),
        Command::CharacterGm { name, enabled } => {
            let project = ProjectLayout::discover()?;
            cmd::character::gm(&project, &runner, &http, &name, enabled).map(|_| EXIT_OK)
        }
        Command::ProductionStatus(options) => {
            let status = cmd::production::inspect(&options, &runner);
            cmd::production::report(&status);
            Ok(if status.blocking() {
                EXIT_FAILURE
            } else {
                EXIT_OK
            })
        }
        Command::ServiceReconcile => {
            cmd::service::reconcile(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK)
        }
        Command::Update => cmd::update::run(&ProjectLayout::discover()?, &runner).map(|_| EXIT_OK),
    }
}

/// The commands the Windows port does not cover, with the name to quote back at the user.
///
/// These shell out to `bash` (`run-suite.sh`, `pull-classic-db.sh`, `import-class-spells.sh`) or to
/// `systemctl`; none exists on a stock Windows install, and porting the world import is a separate
/// job from running the fixture realm.
#[cfg(windows)]
fn unsupported_on_windows(command: &Command) -> Option<&'static str> {
    match command {
        Command::DevSmoke => Some("lyracore dev smoke"),
        Command::ImportWorld(_) => Some("lyracore import world"),
        Command::ImportVmaps { .. } => Some("lyracore import vmaps"),
        Command::ServiceReconcile => Some("lyracore service reconcile"),
        _ => None,
    }
}
