use std::env;
use std::process::{self, Command};

fn main() {
    println!("cargo:rerun-if-env-changed=SDL2_CONFIG");
    println!("cargo:rerun-if-env-changed=SDL2_LIBS");

    if let Some(flags) = env::var("SDL2_LIBS")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        emit_link_flags(&flags);
        return;
    }

    let config = env::var("SDL2_CONFIG").unwrap_or_else(|_| "sdl2-config".to_string());
    for command in [
        vec![config.as_str(), "--libs"],
        vec!["pkg-config", "--libs", "sdl2"],
    ] {
        if let Ok(output) = Command::new(command[0]).args(&command[1..]).output() {
            if output.status.success() {
                let flags = String::from_utf8_lossy(&output.stdout);
                emit_link_flags(&flags);
                return;
            }
        }
    }

    eprintln!("{}", missing_sdl_message(&config));
    process::exit(1);
}

fn emit_link_flags(flags: &str) {
    let mut emitted_anything = false;
    let mut tokens = flags.split_whitespace();
    while let Some(token) = tokens.next() {
        if let Some(path) = token.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={path}");
            emitted_anything = true;
            continue;
        }

        if let Some(path) = token.strip_prefix("-F") {
            println!("cargo:rustc-link-search=framework={path}");
            emitted_anything = true;
            continue;
        }

        if let Some(lib) = token.strip_prefix("-l") {
            println!("cargo:rustc-link-lib={lib}");
            emitted_anything = true;
            continue;
        }

        if token == "-framework" {
            if let Some(name) = tokens.next() {
                println!("cargo:rustc-link-lib=framework={name}");
                emitted_anything = true;
            }
        }
    }

    if !emitted_anything {
        println!("cargo:rustc-link-lib=SDL2");
    }
}

fn missing_sdl_message(config: &str) -> String {
    format!(
        "skyroads-sdl requires the native SDL2 development library.\n\
         \n\
         Tried these detection paths:\n\
           - SDL2_LIBS environment override\n\
           - {config} --libs\n\
           - pkg-config --libs sdl2\n\
         \n\
         Install SDL2 development files, then retry.\n\
         Common package names:\n\
           - Debian/Ubuntu: libsdl2-dev\n\
           - Fedora: SDL2-devel\n\
           - Arch: sdl2\n\
           - macOS (Homebrew): sdl2\n\
         \n\
         If SDL2 is installed in a non-standard location, set one of:\n\
           - SDL2_CONFIG=/absolute/path/to/sdl2-config\n\
           - SDL2_LIBS='-L/absolute/path/to/lib -lSDL2'\n"
    )
}
