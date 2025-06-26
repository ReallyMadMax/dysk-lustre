//! This file is executed during compilation.
//! It builds shell completion scripts and the man page
//! and configures Lustre support
//!
//! Note: to see the eprintln messages, run cargo with
//!     cargo -vv build --release
use {
    dysk_cli::args::Args,
    clap::CommandFactory,
    clap_complete::{Generator, Shell},
    serde::Deserialize,
    toml,
    std::{
        env,
        ffi::OsStr,
        fs,
        path::PathBuf,
        process::Command,
    },
};

fn write_completions_file<G: Generator + Copy, P: AsRef<OsStr>>(generator: G, out_dir: P) {
    let mut args = Args::command();
    clap_complete::generate_to(
        generator,
        &mut args,
        "dysk".to_string(),
        &out_dir,
    ).expect("clap complete generation failed");
}

/// write the shell completion scripts which will be added to
/// the release archive
fn build_completion_scripts() {
    let out_dir = env::var_os("OUT_DIR").expect("out dir not set");
    write_completions_file(Shell::Bash, &out_dir);
    write_completions_file(Shell::Elvish, &out_dir);
    write_completions_file(Shell::Fish, &out_dir);
    write_completions_file(Shell::Zsh, &out_dir);
    eprintln!("completion scripts generated in {out_dir:?}");
}

/// generate the man page from the Clap configuration
fn build_man_page() -> std::io::Result<()> {
    let out_dir = env::var_os("OUT_DIR").expect("out dir not set");
    let out_dir = PathBuf::from(out_dir);
    let cmd = Args::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::<u8>::default();
    man.render(&mut buffer)?;
    let file_path = out_dir.join("dysk.1");
    std::fs::write(&file_path, buffer)?;
    eprintln!("man page generated in {file_path:?}");
    Ok(())
}

/// Check that all dysk versions are the same
///
/// See <https://github.com/Canop/dysk/issues/65>
fn check_version_consistency() -> std::io::Result<()> {
    #[derive(Deserialize)]
    struct Package {
        version: String,
    }
    #[derive(Deserialize)]
    struct DependencyRef {
        version: String,
    }
    #[derive(Deserialize)]
    struct Dependencies {
        #[serde(alias = "dysk-cli")]
        dysk_cli: DependencyRef,
    }
    #[derive(Deserialize)]
    struct MainCargo {
        package: Package,
        dependencies: Dependencies,
        #[serde(alias = "build-dependencies")]
        build_dependencies: Dependencies,
    }
    #[derive(Deserialize)]
    struct CliCargo {
        package: Package,
    }
    let version = env::var("CARGO_PKG_VERSION").expect("cargo pkg version not available");
    let s = fs::read_to_string("Cargo.toml").unwrap();
    let main_cargo: MainCargo = toml::from_str(&s).unwrap();
    let Ok(s) = fs::read_to_string("cli/Cargo.toml") else {
        // won't be visible unless run with -vv
        eprintln!("No local cli/Cargo.toml -- Assuming a cargo publish compilation");
        return Ok(());
    };
    let cli_cargo: CliCargo = toml::from_str(&s).unwrap();
    let ok =
        (version == main_cargo.package.version)
        && (version == main_cargo.dependencies.dysk_cli.version)
        && (version == main_cargo.build_dependencies.dysk_cli.version)
        && (version == cli_cargo.package.version);
    if ok {
        eprintln!("Checked consistency of dysk and dysk-cli versions: OK");
    } else {
        panic!("VERSION MISMATCH - All dysk and dysk-cli versions must be the same");
    }
    Ok(())
}

/// Configure Lustre support
fn configure_lustre_support() {
    // Only process Lustre-related build steps on Linux
    if !cfg!(target_os = "linux") {
        eprintln!("ℹ Skipping Lustre configuration on non-Linux platform");
        return;
    }

    println!("cargo:rerun-if-env-changed=LUSTRE_DIR");
    
    eprintln!("Configuring Lustre support...");
    
    if detect_lustre() {
        configure_lustre_build();
        eprintln!("✓ Lustre support enabled and configured");
    } else {
        eprintln!("⚠ WARNING: Lustre not found on system");
        eprintln!("⚠ WARNING: Falling back to stub implementation");
        eprintln!("⚠ WARNING: Install lustre-client package for full functionality");
        eprintln!("ℹ dysk will still work but without Lustre filesystem discovery");
    }
}

fn detect_lustre() -> bool {
    // Check if lfs command is available
    let lfs_available = Command::new("lfs")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    
    if !lfs_available {
        eprintln!("lfs command not found");
        return false;
    }
    
    eprintln!("lfs command found");
    
    // Try to find liblustreapi
    if find_lustre_library() {
        eprintln!("liblustreapi found");
        true
    } else {
        eprintln!("liblustreapi not found");
        false
    }
}

fn find_lustre_library() -> bool {
    let search_paths = [
        "/usr/lib64",
        "/usr/lib",
        "/usr/local/lib", 
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib64/lustre",
        "/usr/lib/lustre",
    ];
    
    for path in &search_paths {
        let lib_path = PathBuf::from(path).join("liblustreapi.so");
        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", path);
            eprintln!("    Found: {}/liblustreapi.so", path);
            return true;
        }
        
        let static_lib_path = PathBuf::from(path).join("liblustreapi.a");
        if static_lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", path);
            eprintln!("    Found: {}/liblustreapi.a", path);
            return true;
        }
    }
    
    // Try pkg-config as fallback
    if Command::new("pkg-config")
        .args(&["--exists", "lustre"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
    {
        if let Ok(output) = Command::new("pkg-config")
            .args(&["--libs-only-L", "lustre"])
            .output()
        {
            if let Ok(libs) = String::from_utf8(output.stdout) {
                for lib in libs.split_whitespace() {
                    if lib.starts_with("-L") {
                        println!("cargo:rustc-link-search=native={}", &lib[2..]);
                        eprintln!("    Found via pkg-config: {}", &lib[2..]);
                    }
                }
                return true;
            }
        }
    }
    
    false
}

fn configure_lustre_build() {
    println!("cargo:rustc-link-lib=lustreapi");
    println!("cargo:rustc-cfg=lustre_available");
}

fn main() -> std::io::Result<()> {
    check_version_consistency()?;
    build_completion_scripts();
    build_man_page()?;
    configure_lustre_support();
    
    eprintln!("dysk-lustre build script completed");
    
    Ok(())
}