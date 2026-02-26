use assert_cmd::Command;
use std::io::Write;
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

/// Get path to a workspace member binary (uses deprecated function because
/// CARGO_BIN_EXE_* env vars only work for same-package binaries)
#[allow(deprecated)]
fn workspace_bin(name: &str) -> PathBuf {
    assert_cmd::cargo::cargo_bin(name)
}

#[test]
fn test_eval_commands_flag() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", "1 + 2"])
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn test_eval_file() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "3 + 4").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout("7\n");
}

#[test]
fn test_eval_stdin() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-"])
        .write_stdin("5 + 6")
        .assert()
        .success()
        .stdout("11\n");
}

#[test]
fn test_eval_mj_compile() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", r#".mj compile --inline "test" | describe"#])
        .assert()
        .success()
        .stdout("CompiledTemplate\n");
}

#[test]
fn test_eval_mj_compile_and_render() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "eval",
            "-c",
            r#"let tpl = (.mj compile --inline "Hi {{ name }}"); {name: "World"} | .mj render $tpl"#,
        ])
        .assert()
        .success()
        .stdout("Hi World\n");
}

#[test]
fn test_eval_print() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["--log-format", "jsonl", "eval", "-c", r#"print "hello""#])
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""message":"print""#))
        .stdout(predicates::str::contains(r#""content":"hello""#));
}

#[test]
fn test_http_nu_const_defaults() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", "$HTTP_NU.dev"])
        .assert()
        .success()
        .stdout("false\n");
}

#[test]
fn test_http_nu_const_dev_flag() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["--dev", "eval", "-c", "$HTTP_NU.dev"])
        .assert()
        .success()
        .stdout("true\n");
}

#[test]
fn test_http_nu_const_is_immutable() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", "$HTTP_NU = {}"])
        .assert()
        .failure();
}

#[test]
fn test_eval_syntax_error() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", "1 +"])
        .assert()
        .failure();
}

#[test]
fn test_eval_no_input() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "provide a file or use --commands",
        ));
}

#[test]
fn test_eval_both_file_and_commands() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", "1", "file.nu"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot specify both"));
}

#[test]
fn test_eval_with_plugin() {
    let plugin_path = workspace_bin("nu_plugin_test");
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "eval",
            "--plugin",
            plugin_path.to_str().unwrap(),
            "-c",
            "test-plugin-cmd",
        ])
        .assert()
        .success()
        .stdout("PLUGIN_WORKS\n");
}

#[test]
fn test_eval_include_path() {
    let dir = TempDir::new().unwrap();
    let module_path = dir.path().join("mymod.nu");
    std::fs::write(&module_path, "export def hello [] { 'world' }").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "-I",
            dir.path().to_str().unwrap(),
            "eval",
            "-c",
            "use mymod.nu; mymod hello",
        ])
        .assert()
        .success()
        .stdout("world\n");
}

#[test]
fn test_eval_include_path_multiple() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    std::fs::write(dir1.path().join("mod1.nu"), "export def a [] { 1 }").unwrap();
    std::fs::write(dir2.path().join("mod2.nu"), "export def b [] { 2 }").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "-I",
            dir1.path().to_str().unwrap(),
            "-I",
            dir2.path().to_str().unwrap(),
            "eval",
            "-c",
            "use mod1.nu; use mod2.nu; (mod1 a) + (mod2 b)",
        ])
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn test_mj_file_with_external_refs() {
    // .mj "file" with extends/include - works because loader is set up
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "eval",
            "-c",
            r#"{name: "World"} | .mj "examples/templates/page.html""#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Page (from disk)</nav>"))
        .stdout(predicates::str::contains("<title>My Page (disk)</title>"))
        .stdout(predicates::str::contains("Hello World"));
}

#[test]
fn test_mj_inline_is_self_contained() {
    // Inline mode has no loader -- {% include %} references fail
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .current_dir("examples/templates")
        .args([
            "eval",
            "-c",
            r#"{} | .mj --inline '{% include "nav.html" %}'"#,
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("template not found"));
}

#[test]
fn test_mj_compile_file_with_external_refs() {
    // .mj compile "file" + render - loader is preserved in cache
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args([
            "eval",
            "-c",
            r#"let t = .mj compile "examples/templates/page.html"; {name: "World"} | .mj render $t"#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Page (from disk)</nav>"))
        .stdout(predicates::str::contains("<title>My Page (disk)</title>"))
        .stdout(predicates::str::contains("Hello World"));
}

#[test]
fn test_mj_compile_inline_is_self_contained() {
    // Compile + render in inline mode has no loader -- {% include %} fails
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .current_dir("examples/templates")
        .args([
            "eval",
            "-c",
            r#"let t = .mj compile --inline '{% include "nav.html" %}'; {} | .mj render $t"#,
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("template not found"));
}

#[test]
fn test_mj_topic_without_store_fails() {
    // --topic requires --store; without it the command should fail
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", r#"{} | .mj --topic "some.topic""#])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--topic requires --store"));
}

#[test]
fn test_mj_compile_topic_without_store_fails() {
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", r#".mj compile --topic "some.topic""#])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--topic requires --store"));
}

#[test]
fn test_mj_rejects_combined_modes() {
    // Cannot combine --inline and --topic
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", r#"{} | .mj --inline "hi" --topic "t""#])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Cannot combine"));
}

#[test]
fn test_mj_compile_rejects_combined_modes() {
    // Cannot combine file and --topic
    Command::new(assert_cmd::cargo::cargo_bin!("http-nu"))
        .args(["eval", "-c", r#".mj compile "file.html" --topic "t""#])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Cannot combine"));
}
