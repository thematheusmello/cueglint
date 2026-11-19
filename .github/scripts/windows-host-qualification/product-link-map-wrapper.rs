// Cargo's workspace wrapper receives the actual rustc executable as its first argument.
use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File, Metadata},
    io::Write,
    path::{Component, Path, PathBuf},
    process::{self, Command},
};

const TARGET: &str = "x86_64-pc-windows-msvc";
const MAP_DIR: &str = "cueglint-link-maps";

fn flag<'a>(args: &'a [OsString], name: &str) -> Result<Option<&'a OsStr>, &'static str> {
    let mut found = None;
    for (index, arg) in args.iter().enumerate() {
        let value = if arg == name {
            Some(
                args.get(index + 1)
                    .filter(|value| !value.to_string_lossy().starts_with('-'))
                    .ok_or("missing rustc flag value")?
                    .as_os_str(),
            )
        } else {
            arg.to_str()
                .and_then(|arg| arg.strip_prefix(name)?.strip_prefix('='))
                .map(OsStr::new)
        };
        if let Some(value) = value {
            if value.is_empty() || found.replace(value).is_some() {
                return Err("ambiguous rustc flag");
            }
        }
    }
    Ok(found)
}

fn selected(
    args: &[OsString],
    primary: bool,
    package: Option<&str>,
    bin: Option<&str>,
) -> Result<Option<(&'static str, &'static str)>, &'static str> {
    let (name, crate_name) = match (primary, package, bin) {
        (true, Some("cueglint"), Some("cueglint")) => ("cueglint", "cueglint"),
        (true, Some("cueglint-cli"), Some("cueglint-cli")) => ("cueglint-cli", "cueglint_cli"),
        _ => return Ok(None),
    };
    if args.iter().any(|arg| {
        arg == "--test"
            || arg == "--print"
            || arg.to_str().is_some_and(|arg| arg.starts_with("--print="))
    }) || flag(args, "--target")? != Some(OsStr::new(TARGET))
    {
        return Ok(None);
    }
    if flag(args, "--crate-type")? != Some(OsStr::new("bin"))
        || flag(args, "--crate-name")? != Some(OsStr::new(crate_name))
    {
        return Err("unexpected product crate name or type");
    }
    let Some(emit) = flag(args, "--emit")?.and_then(OsStr::to_str) else {
        return Ok(None);
    };
    let links = emit.split(',').filter(|kind| *kind == "link").count();
    if emit.split(',').any(|kind| kind.starts_with("link=")) || links > 1 {
        return Err("custom product output is unsupported");
    }
    if links == 0 {
        return Ok(None);
    }
    if args.iter().any(|arg| {
        arg == "--"
            || arg
                .to_str()
                .is_some_and(|arg| arg.starts_with("-o") || arg.starts_with('@'))
    }) {
        return Err("custom product output is unsupported");
    }
    Ok(Some((name, crate_name)))
}

fn internal_stem(crate_name: &str, args: &[OsString]) -> Result<String, &'static str> {
    let mut suffix = None;
    for (index, arg) in args.iter().enumerate() {
        let Some(option) = arg.to_str().and_then(|arg| match arg {
            "-C" | "--codegen" => args.get(index + 1)?.to_str(),
            _ => arg
                .strip_prefix("-C")
                .or_else(|| arg.strip_prefix("--codegen=")),
        }) else {
            continue;
        };
        if let Some(value) = option.strip_prefix("extra-filename=") {
            if suffix.replace(value).is_some() {
                return Err("ambiguous rustc artifact suffix");
            }
        }
        if let Some((key, value)) = option.split_once('=') {
            if matches!(key, "link-arg" | "link-args")
                && value.split_ascii_whitespace().any(|value| {
                    value
                        .trim_start_matches('"')
                        .to_ascii_lowercase()
                        .starts_with("/map")
                })
            {
                return Err("existing linker map flag");
            }
        }
    }
    let Some(suffix) = suffix else {
        return Err("missing rustc artifact suffix");
    };
    if suffix.len() != 17
        || !suffix.starts_with('-')
        || !suffix[1..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid rustc artifact suffix");
    }
    Ok(format!("{crate_name}{suffix}"))
}

fn metadata(path: &Path) -> Result<Metadata, &'static str> {
    let info = fs::symlink_metadata(path).map_err(|_| "missing map or artifact path")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if info.file_attributes() & 0x400 != 0 {
            return Err("reparse point in evidence path");
        }
    }
    if info.file_type().is_symlink() {
        return Err("symlink in evidence path");
    }
    Ok(info)
}

fn run() -> Result<i32, &'static str> {
    let mut arguments = env::args_os().skip(1);
    let compiler = arguments.next().ok_or("missing rustc executable")?;
    let args: Vec<_> = arguments.collect();
    let mut command = Command::new(compiler);
    command.args(&args);
    let primary = env::var("CARGO_PRIMARY_PACKAGE").as_deref() == Ok("1");
    let package = env::var("CARGO_PKG_NAME").ok();
    let bin = env::var("CARGO_BIN_NAME").ok();
    let Some((name, crate_name)) = selected(&args, primary, package.as_deref(), bin.as_deref())?
    else {
        return Ok(command
            .status()
            .map_err(|_| "could not launch rustc")?
            .code()
            .unwrap_or(1));
    };

    let root =
        PathBuf::from(env::var_os("CARGO_TARGET_DIR").ok_or("missing private Cargo target")?);
    if !root.is_absolute()
        || root.components().any(|part| part == Component::ParentDir)
        || !metadata(&root)?.is_dir()
    {
        return Err("invalid private Cargo target");
    }
    let deps = root.join(TARGET).join("release").join("deps");
    if flag(&args, "--out-dir")?.map(Path::new) != Some(deps.as_path())
        || !metadata(&deps)?.is_dir()
    {
        return Err("unexpected rustc output directory");
    }
    let stem = internal_stem(crate_name, &args)?;
    let map_dir = root.join(MAP_DIR);
    match fs::create_dir(&map_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("could not create private map directory"),
    }
    if !metadata(&map_dir)?.is_dir() {
        return Err("invalid private map directory");
    }
    let map = map_dir.join(format!("{name}.map"));
    let executable = deps.join(format!("{stem}.exe"));
    let mut claim = File::options()
        .write(true)
        .create_new(true)
        .open(map_dir.join(format!("{name}.linked")))
        .map_err(|_| "duplicate product link")?;
    for output in [&map, &executable] {
        match fs::symlink_metadata(output) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("product link output path is already occupied"),
        }
    }
    let mut map_arg = OsString::from("link-arg=/MAP:");
    map_arg.push(&map);
    let status = command
        .arg("-C")
        .arg(map_arg)
        .status()
        .map_err(|_| "could not launch rustc")?;
    if status.success() {
        let map_info = metadata(&map)?;
        let executable = metadata(&executable)?;
        if !map_info.is_file() || map_info.len() == 0 || !executable.is_file() {
            return Err("product link outputs missing");
        }
        writeln!(claim, "{stem}").map_err(|_| "could not complete product link marker")?;
    }
    Ok(status.code().unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => process::exit(code),
        Err(reason) => {
            eprintln!("product link map wrapper: {reason}");
            process::exit(1);
        }
    }
}
