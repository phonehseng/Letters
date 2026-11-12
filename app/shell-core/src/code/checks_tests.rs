// tests of the compiler-backed checks: only syntax-check flags are used, a
// missing checker is said plainly, and the person's program never runs.
// the tests that need a real compiler skip themselves when none is found.

use super::*;
use crate::code::tools::{find_tools, this_computers_folders};

fn arguments_as_text(kind: ToolKind) -> Vec<String> {
    let file = Path::new("/tmp/check/main.c");
    let temp = Path::new("/tmp/check");
    check_arguments(kind, file, temp, Some(Path::new("/home/sam/project")))
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn c_compilers_are_only_ever_asked_for_a_syntax_check() {
    for kind in [
        ToolKind::Gcc,
        ToolKind::Gxx,
        ToolKind::Clang,
        ToolKind::ClangXx,
    ] {
        let arguments = arguments_as_text(kind);
        assert!(
            arguments.contains(&"-fsyntax-only".to_owned()),
            "{arguments:?}"
        );
        for building_flag in ["-o", "-c", "-E", "-S", "-run"] {
            assert!(
                !arguments.contains(&building_flag.to_owned()),
                "{kind:?} {arguments:?}"
            );
        }
    }
}

#[test]
fn javac_writes_only_into_the_temp_folder_and_runs_no_processors() {
    let arguments = arguments_as_text(ToolKind::Javac);
    let output_folder = arguments
        .iter()
        .position(|a| a == "-d")
        .map(|i| &arguments[i + 1]);
    assert_eq!(output_folder.map(String::as_str), Some("/tmp/check"));
    assert!(arguments.contains(&"-proc:none".to_owned()));
}

#[test]
fn python_and_node_only_parse() {
    let python = arguments_as_text(ToolKind::Python);
    assert!(python.iter().any(|a| a.contains("ast.parse")));
    assert!(!python
        .iter()
        .any(|a| a.contains("exec(") || a.contains("runpy")));
    assert_eq!(arguments_as_text(ToolKind::Node)[0], "--check");
}

#[test]
fn a_missing_checker_is_a_sentence_not_an_error() {
    let request = CheckRequest {
        language: CheckLanguage::C,
        text: "int main(void) { return 0; }".into(),
        path: None,
    };
    let outcome = run_check(&request, &[], &|| true).unwrap();
    assert_eq!(outcome.checker, None);
    assert!(outcome.note.unwrap().contains("no C compiler"));
}

#[test]
fn odd_file_names_are_not_reused_for_the_copy() {
    let request = |path: &str| CheckRequest {
        language: CheckLanguage::C,
        text: String::new(),
        path: Some(path.into()),
    };
    assert_eq!(copy_name(&request("/home/sam/hello.c")), "hello.c");
    assert_eq!(copy_name(&request("/home/sam/-o evil.c")), "untitled.c");
}

/// a tool of this kind on this computer, or none (the test then skips).
fn installed(kind: ToolKind) -> Option<FoundTool> {
    find_tools(&this_computers_folders())
        .into_iter()
        .find(|tool| tool.kind == kind)
}

#[test]
fn gcc_reports_a_missing_semicolon_without_building_or_running_anything() {
    let Some(gcc) = installed(ToolKind::Gcc) else {
        eprintln!("skipped: gcc is not installed");
        return;
    };
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("hello.c");
    let marker = project.path().join("ran.txt");
    // if this program ever ran, it would leave ran.txt beside it
    let text = format!(
        "#include <stdio.h>\nint main(void) {{\n  FILE *f = fopen(\"{}\", \"w\");\n  int x = y;\n  int z = 2\n  return x;\n}}\n",
        marker.to_string_lossy().replace('\\', "/")
    );
    fs::write(&source, &text).unwrap();
    let request = CheckRequest {
        language: CheckLanguage::C,
        text,
        path: Some(source.to_string_lossy().into_owned()),
    };
    let outcome = run_check(&request, std::slice::from_ref(&gcc), &|| true).unwrap();
    assert!(outcome.output.contains("hello.c:6:"), "{}", outcome.output);
    // gcc quotes the name with plain quotes, or with curly ones when the
    // system's language uses utf-8 (as on ubuntu): either is fine to show
    assert!(
        outcome.output.contains("'y' undeclared") || outcome.output.contains("‘y’ undeclared"),
        "{}",
        outcome.output
    );
    assert!(!marker.exists());
    let left_beside: Vec<_> = fs::read_dir(project.path()).unwrap().collect();
    assert_eq!(
        left_beside.len(),
        1,
        "only the source file stays in its folder"
    );
}

#[test]
fn python_reports_a_syntax_error_and_never_runs_the_file() {
    let Some(python) = installed(ToolKind::Python) else {
        eprintln!("skipped: python is not installed");
        return;
    };
    let project = tempfile::tempdir().unwrap();
    let marker = project.path().join("ran.txt");
    let text = format!(
        "open(r'{}', 'w').write('ran')\nif True\n    print('hi')\n",
        marker.to_string_lossy()
    );
    let request = CheckRequest {
        language: CheckLanguage::Python,
        text,
        path: None,
    };
    let outcome = run_check(&request, std::slice::from_ref(&python), &|| true).unwrap();
    assert!(
        outcome.output.contains("untitled.py:2:"),
        "{}",
        outcome.output
    );
    assert!(outcome.output.contains("error:"), "{}", outcome.output);
    assert!(!marker.exists());
}

#[test]
fn each_check_gets_its_own_temp_folder_and_it_is_removed_afterwards() {
    let first = TempCheckFolder::create().unwrap();
    let second = TempCheckFolder::create().unwrap();
    assert_ne!(first.path, second.path);
    assert!(first.path.starts_with(std::env::temp_dir()));
    let used = first.path.clone();
    drop(first);
    assert!(!used.exists());
}

#[test]
fn a_clean_file_gives_gcc_nothing_to_say() {
    let Some(gcc) = installed(ToolKind::Gcc) else {
        eprintln!("skipped: gcc is not installed");
        return;
    };
    let request = CheckRequest {
        language: CheckLanguage::C,
        text: "int main(void) { return 0; }\n".into(),
        path: None,
    };
    let outcome = run_check(&request, std::slice::from_ref(&gcc), &|| true).unwrap();
    assert_eq!(outcome.output.trim(), "");
    assert_eq!(
        outcome
            .checker
            .as_deref()
            .map(|label| label.starts_with("gcc ")),
        Some(true)
    );
}
