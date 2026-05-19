use hakana_code_info::analysis_result::{
    AnalysisResult, CheckPointEntry, FullEntry, HhClientEntry,
};
use hakana_str::Interner;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

pub(crate) fn update_files(
    analysis_result: &mut AnalysisResult,
    root_dir: &String,
    interner: &Interner,
) {
    use std::collections::BTreeMap;

    let files_with_edits = analysis_result.files_with_edits();

    let sorted_files: BTreeMap<String, _> = files_with_edits
        .into_iter()
        .map(|fp| (fp.get_relative_path(interner, root_dir), fp))
        .collect();

    for (relative_path, original_path) in sorted_files {
        tty_println!("updating {}", relative_path);
        let file_path = format!("{}/{}", root_dir, relative_path);
        let file_contents = fs::read_to_string(&file_path).unwrap();
        let mut file = File::create(&file_path).unwrap();

        let edit_set = analysis_result.take_edits_for_file(&original_path);

        let new_contents = edit_set
            .apply(&file_contents)
            .unwrap_or_else(|e| panic!("Failed to apply edits to {}: {}", &file_path, e));

        file.write_all(new_contents.as_bytes())
            .unwrap_or_else(|_| panic!("Could not write file {}", &file_path));
    }
}

pub(crate) fn write_analysis_output_files(
    output_file: String,
    output_format: Option<String>,
    cwd: &String,
    analysis_result: &AnalysisResult,
    interner: &Interner,
) {
    let output_path = if output_file.starts_with('/') {
        output_file
    } else {
        format!("{}/{}", cwd, output_file)
    };
    let mut output_path = fs::File::create(Path::new(&output_path)).unwrap();

    let json = match output_format {
        Some(format) if format == "full" => {
            let mut entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    entries.push(FullEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&entries).unwrap()
        }
        Some(format) if format == "hh_client" => {
            let mut entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    entries.push(HhClientEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&entries).unwrap()
        }
        _ => {
            let mut checkpoint_entries = vec![];

            for (file_path, issues) in analysis_result.get_all_issues(interner, cwd, true) {
                for issue in issues {
                    checkpoint_entries.push(CheckPointEntry::from_issue(issue, &file_path));
                }
            }

            serde_json::to_string_pretty(&checkpoint_entries).unwrap()
        }
    };
    write!(output_path, "{}", json).unwrap();
}
