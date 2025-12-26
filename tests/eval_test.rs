use assert_cmd::Command;
use std::io::Write;
use tempfile::NamedTempFile;

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
