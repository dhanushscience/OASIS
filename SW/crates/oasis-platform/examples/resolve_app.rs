//! Show what app names resolve to on this machine, without starting anything:
//! `cargo run -p oasis-platform --example resolve_app -- spotify "vs code" calculator`.

#[cfg(windows)]
fn main() {
    use oasis_core::actions::app_resolver::resolve;
    use oasis_platform::windows::app_discovery::{start_command, windows_apps};

    let names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        println!("Give one or more app names.");
        return;
    }
    for name in names {
        let lookup = oasis_core::voice::command::apps::launch_name(&name).unwrap_or(&name);
        match resolve(lookup, windows_apps()) {
            Some(found) => println!("{name:<22} -> {found:?}\n{:<22}    starts: {:?}", "", start_command(&found)),
            None => println!("{name:<22} -> nothing"),
        }
    }
}

#[cfg(not(windows))]
fn main() {
    println!("App discovery is only built for Windows so far.");
}
