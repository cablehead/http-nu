use assert_cmd::cargo::cargo_bin;
use assert_cmd::Command;
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

#[test]
fn test_eval_commands_flag() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", "-c", "1 + 2"])
        .assert()
        .success()
        .stdout("3\n");
}

#[test]
fn test_eval_file() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(file, "3 + 4").unwrap();

    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", file.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout("7\n");
}

#[test]
fn test_eval_stdin() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", "-"])
        .write_stdin("5 + 6")
        .assert()
        .success()
        .stdout("11\n");
}

#[test]
fn test_eval_mj_compile() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", "-c", r#".mj compile --inline "test" | describe"#])
        .assert()
        .success()
        .stdout("CompiledTemplate\n");
}

#[test]
fn test_eval_mj_compile_and_render() {
    Command::cargo_bin("http-nu")
        .unwrap()
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
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["--log-format", "jsonl", "eval", "-c", r#"print "hello""#])
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""message":"print""#))
        .stdout(predicates::str::contains(r#""content":"hello""#));
}

#[test]
fn test_eval_syntax_error() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", "-c", "1 +"])
        .assert()
        .failure();
}

#[test]
fn test_eval_no_input() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "provide a file or use --commands",
        ));
}

#[test]
fn test_eval_both_file_and_commands() {
    Command::cargo_bin("http-nu")
        .unwrap()
        .args(["eval", "-c", "1", "file.nu"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("cannot specify both"));
}

#[test]
fn test_eval_with_plugin() {
    let plugin_path = cargo_bin("nu_plugin_test");
    Command::cargo_bin("http-nu")
        .unwrap()
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

    Command::cargo_bin("http-nu")
        .unwrap()
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

    Command::cargo_bin("http-nu")
        .unwrap()
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
    Command::cargo_bin("http-nu")
        .unwrap()
        .args([
            "eval",
            "-c",
            r#"{name: "World"} | .mj "examples/template-inheritance/page.html""#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("<nav>Home | About</nav>"))
        .stdout(predicates::str::contains("<title>My Page</title>"))
        .stdout(predicates::str::contains("Hello World"));
}

#[test]
fn test_mj_inline_with_external_refs() {
    // .mj --inline with include - FAILS: no loader for cwd
    Command::cargo_bin("http-nu")
        .unwrap()
        .current_dir("examples/template-inheritance")
        .args([
            "eval",
            "-c",
            r#"{} | .mj --inline '{% include "nav.html" %}'"#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("<nav>Home | About</nav>"));
}

#[test]
fn test_mj_compile_file_with_external_refs() {
    // .mj compile "file" + render - FAILS: loader not preserved in cache
    Command::cargo_bin("http-nu")
        .unwrap()
        .args([
            "eval",
            "-c",
            r#"let t = .mj compile "examples/template-inheritance/page.html"; {name: "World"} | .mj render $t"#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("<nav>Home | About</nav>"))
        .stdout(predicates::str::contains("<title>My Page</title>"))
        .stdout(predicates::str::contains("Hello World"));
}

#[test]
fn test_mj_compile_inline_with_external_refs() {
    // .mj compile --inline + render - FAILS: no loader for cwd
    Command::cargo_bin("http-nu")
        .unwrap()
        .current_dir("examples/template-inheritance")
        .args([
            "eval",
            "-c",
            r#"let t = .mj compile --inline '{% include "nav.html" %}'; {} | .mj render $t"#,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("<nav>Home | About</nav>"));
}
