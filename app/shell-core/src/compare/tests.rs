//! the compare job run against the real pdf engine: progress, the result,
//! stopping, the pictures, the report, and that nothing stays open after.

use std::time::{Duration, Instant};

use letters_compare::{CompareOptions, ReportOptions};
use letters_pdf::PageSize;

use super::test_files::{locked_pdf_bytes, numbered_page, pdf_bytes, start_worker, write, Page};
use super::*;
use crate::test_pdf::temp_dir;

fn side(path: &std::path::Path, name: &str) -> SideSpec {
    SideSpec {
        path: path.to_owned(),
        name: name.to_owned(),
    }
}

/// waits until the job is no longer running.
fn finish(job: &CompareJob) -> JobStatus {
    let start = Instant::now();
    loop {
        let status = job.status();
        if !matches!(status, JobStatus::Running { .. }) {
            return status;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "the comparison never finished"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn three_pages(changed_word: &str) -> Vec<Page> {
    vec![
        numbered_page(1),
        Page::default().text(
            72.0,
            100.0,
            12.0,
            &format!("The quick brown {changed_word} jumps over the lazy dog."),
        ),
        numbered_page(3),
    ]
}

#[test]
fn a_job_reports_every_page_and_finds_a_changed_word() {
    let dir = temp_dir("compare-job-word");
    let worker = start_worker();
    let old = write(&dir, "old.pdf", &pdf_bytes(&three_pages("fox")));
    let new = write(&dir, "new.pdf", &pdf_bytes(&three_pages("cat")));
    let job = CompareJob::start(
        &worker,
        1,
        &side(&old, "old.pdf"),
        &side(&new, "new.pdf"),
        CompareOptions::default(),
        dir.join("job"),
    )
    .unwrap();

    let status = finish(&job);

    let JobStatus::Finished { summary } = status else {
        panic!("expected a finished comparison, got {status:?}");
    };
    assert_eq!(summary.text, 1);
    let result = job.result().unwrap();
    let change = &result.page_pairs[1].changes[0];
    assert_eq!(change.old_text.as_deref(), Some("fox"));
    assert_eq!(change.new_text.as_deref(), Some("cat"));
    // three pages read on each side, then three pairs compared
    assert_eq!(job.progress_reports(), 9);
    assert_eq!(job.old.page_sizes.len(), 3);
    assert_eq!(job.new.name, "new.pdf");
}

#[test]
fn identical_documents_written_differently_have_no_changes() {
    let dir = temp_dir("compare-job-same");
    let worker = start_worker();
    let pages = three_pages("fox");
    let old = write(&dir, "old.pdf", &pdf_bytes(&pages));
    // the same pages with a comment added at the end: other bytes, same document
    let mut bytes = pdf_bytes(&pages);
    bytes.extend_from_slice(b"% written again\n");
    let new = write(&dir, "new.pdf", &bytes);
    let job = CompareJob::start(
        &worker,
        2,
        &side(&old, "a"),
        &side(&new, "b"),
        CompareOptions::default(),
        dir.join("job"),
    )
    .unwrap();

    assert!(matches!(finish(&job), JobStatus::Finished { summary } if summary.total == 0));
}

#[test]
fn a_job_can_be_stopped() {
    let dir = temp_dir("compare-job-cancel");
    let worker = start_worker();
    let old_pages: Vec<Page> = (0..120).map(numbered_page).collect();
    let new_pages: Vec<Page> = (0..120).map(|seed| numbered_page(seed + 1000)).collect();
    let old = write(&dir, "old.pdf", &pdf_bytes(&old_pages));
    let new = write(&dir, "new.pdf", &pdf_bytes(&new_pages));
    let job = CompareJob::start(
        &worker,
        3,
        &side(&old, "a"),
        &side(&new, "b"),
        CompareOptions::default(),
        dir.join("job"),
    )
    .unwrap();

    job.cancel();

    assert_eq!(finish(&job), JobStatus::Cancelled);
    assert!(job.result().is_none());
}

#[test]
fn closing_a_job_closes_its_documents_and_deletes_its_folder() {
    let dir = temp_dir("compare-job-close");
    let worker = start_worker();
    let before = worker.stats().unwrap().open_documents;
    let jobs = CompareJobs::new(dir.join("comparisons"));
    let (id, folder) = jobs.prepare().unwrap();
    let old = write(&dir, "old.pdf", &pdf_bytes(&three_pages("fox")));
    // the open document side is a copy in the job's folder, as the shell makes it
    let working = write(&dir, "working.pdf", &pdf_bytes(&three_pages("cat")));
    let copy = copy_for_comparison(&working, &folder).unwrap();
    let job = CompareJob::start(
        &worker,
        id,
        &side(&old, "old"),
        &side(&copy, "new"),
        CompareOptions::default(),
        folder.clone(),
    )
    .unwrap();
    jobs.insert("main", job);
    assert_eq!(worker.stats().unwrap().open_documents, before + 2);

    jobs.close_window("main");

    assert_eq!(worker.stats().unwrap().open_documents, before);
    assert!(!folder.exists());
    assert_eq!(jobs.open_count(), 0);
    assert!(jobs.get(id).is_err());
}

#[test]
fn a_locked_file_gets_the_engines_message_and_nothing_stays_open() {
    let dir = temp_dir("compare-job-locked");
    let worker = start_worker();
    let before = worker.stats().unwrap().open_documents;
    let old = write(&dir, "old.pdf", &pdf_bytes(&three_pages("fox")));
    let locked = write(&dir, "locked.pdf", &locked_pdf_bytes());

    let refused = CompareJob::start(
        &worker,
        4,
        &side(&old, "old"),
        &side(&locked, "locked"),
        CompareOptions::default(),
        dir.join("job"),
    );

    assert_eq!(
        refused.err().as_deref(),
        Some("This PDF is locked with a password. Unlock it first, then compare again.")
    );
    assert_eq!(worker.stats().unwrap().open_documents, before);
}

#[test]
fn whole_pages_are_stitched_from_tiles() {
    let dir = temp_dir("compare-render");
    let worker = start_worker();
    // a red box near the bottom right, past the first tile at 2 pixels per point
    let page = Page::default().filled([400.0, 600.0, 100.0, 100.0], [1.0, 0.0, 0.0]);
    let path = write(&dir, "box.pdf", &pdf_bytes(&[page]));
    let info = worker.open(&path, None).unwrap();

    let image = render_page(&worker, info.id, info.page_sizes[0], 0, 2.0).unwrap();

    assert_eq!((image.width, image.height), (1224, 1584));
    assert_eq!(image.pixel(900, 1300), [255, 0, 0, 255]);
    assert_eq!(image.pixel(100, 100)[..3], [255, 255, 255]);
    worker.close(info.id);
}

#[test]
fn pair_pictures_line_up_and_the_report_is_a_pdf() {
    let dir = temp_dir("compare-pictures");
    let worker = start_worker();
    let old = write(&dir, "old.pdf", &pdf_bytes(&three_pages("fox")));
    let new = write(&dir, "new.pdf", &pdf_bytes(&three_pages("cat")));
    let job = CompareJob::start(
        &worker,
        5,
        &side(&old, "old"),
        &side(&new, "new"),
        CompareOptions::default(),
        dir.join("job"),
    )
    .unwrap();
    finish(&job);

    let pictures = job.pair_pictures(1, 1.0).unwrap();
    let options = ReportOptions {
        old_name: "old.pdf".into(),
        new_name: "new.pdf".into(),
        ..ReportOptions::default()
    };
    let pdf = job.report(ReportFormat::Pdf, &options).unwrap();
    let text = String::from_utf8(job.report(ReportFormat::Text, &options).unwrap()).unwrap();

    assert_eq!((pictures.old.width, pictures.old.height), (612, 792));
    assert_eq!((pictures.overlay.width, pictures.new.height), (612, 792));
    let coloured = (0..pictures.overlay.height)
        .flat_map(|y| (0..pictures.overlay.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b, _] = pictures.overlay.pixel(x, y);
            r.abs_diff(g) > 60 || g.abs_diff(b) > 60
        })
        .count();
    assert!(
        coloured > 20,
        "the overlay shows removed and added words in colour"
    );
    assert!(pdf.starts_with(b"%PDF"));
    let report_path = write(&dir, "report.pdf", &pdf);
    assert!(
        worker.open(&report_path, None).is_ok(),
        "the report opens in the engine"
    );
    assert!(text.contains("fox"), "{text}");
}

#[test]
fn big_pages_are_drawn_smaller_than_the_limit() {
    let poster = PageSize {
        width_pt: 3000.0,
        height_pt: 4000.0,
    };
    let scale = safe_scale(poster, 2.0, 4_000_000.0);
    assert!(poster.width_pt * poster.height_pt * scale * scale <= 4_000_001.0);
    assert_eq!(
        safe_scale(
            PageSize {
                width_pt: 612.0,
                height_pt: 792.0
            },
            1.5,
            4_000_000.0
        ),
        1.5
    );
}
