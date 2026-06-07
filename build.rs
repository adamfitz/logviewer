fn cmd(args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".into())
        .trim()
        .to_string()
}

fn main() {
    println!(
        "cargo:rustc-env=GIT_COMMIT={}",
        cmd(&["rev-parse", "--short", "HEAD"])
    );
    println!(
        "cargo:rustc-env=GIT_TAG={}",
        cmd(&["describe", "--tags", "--abbrev=0"])
    );
}
