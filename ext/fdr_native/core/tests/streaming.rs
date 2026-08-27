use fdr_core::{GrepConfig, GrepFormat, GrepPosition, SearchConfig, grep_stream, search_stream};
use std::os::unix::ffi::OsStrExt;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tempfile::TempDir;

/// Wide enough that the streaming walk cannot take its serial pre-scan.
const WIDE: usize = 64;

#[test]
fn search_stream_emits_matching_paths() {
    let directory = TempDir::new().expect("should create temp directory");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.rb");
    std::fs::write(&first, "").expect("should write first file");
    std::fs::write(&second, "").expect("should write second file");
    let config = SearchConfig {
        paths: vec![directory.path().to_path_buf()],
        file_type: vec!["f".to_owned()],
        extension: vec!["txt".to_owned()],
        ..SearchConfig::default()
    };
    let results = Mutex::new(Vec::with_capacity(1));

    search_stream(&config, &AtomicBool::new(false), |paths| {
        results
            .lock()
            .expect("results lock should work")
            .extend(paths);
        true
    })
    .expect("streaming search should succeed");

    assert_eq!(
        *results.lock().expect("results lock should work"),
        vec![first.as_os_str().as_bytes().to_vec()]
    );
}

#[test]
fn search_stream_stops_the_walk_on_an_early_stop() {
    let directory = TempDir::new().expect("should create temp directory");
    for index in 0..WIDE {
        let child = directory.path().join(format!("dir_{index:04}"));
        std::fs::create_dir(&child).expect("should create directory");
        std::fs::write(child.join("file.txt"), "").expect("should write file");
    }
    let config = SearchConfig {
        paths: vec![directory.path().to_path_buf()],
        ..SearchConfig::default()
    };
    let emitted = Mutex::new(0_usize);

    search_stream(&config, &AtomicBool::new(false), |paths| {
        *emitted.lock().expect("emitted lock should work") += paths.len();
        false
    })
    .expect("stopping the stream should succeed");

    let emitted = *emitted.lock().expect("emitted lock should work");
    assert!(
        emitted < WIDE * 2,
        "the walk should stop early, got {emitted} of {} entries",
        WIDE * 2
    );
}

#[test]
fn search_stream_takes_the_serial_path_for_a_small_tree() {
    let directory = TempDir::new().expect("should create temp directory");
    for index in 0..4 {
        std::fs::write(directory.path().join(format!("f{index}.txt")), "")
            .expect("should write file");
    }
    let config = SearchConfig {
        paths: vec![directory.path().to_path_buf()],
        file_type: vec!["f".to_owned()],
        ..SearchConfig::default()
    };
    let batches = Mutex::new(Vec::with_capacity(1));

    search_stream(&config, &AtomicBool::new(false), |paths| {
        batches
            .lock()
            .expect("batches lock should work")
            .push(paths.len());
        true
    })
    .expect("streaming search should succeed");

    assert_eq!(*batches.lock().expect("batches lock should work"), vec![4]);
}

#[test]
fn grep_stream_emits_each_matching_line() {
    let directory = TempDir::new().expect("should create temp directory");
    let path = directory.path().join("content.txt");
    std::fs::write(&path, "needle\nhaystack\nneedle twice needle\n").expect("should write file");
    let config = GrepConfig {
        pattern: "needle".to_owned(),
        search: SearchConfig {
            paths: vec![path.clone()],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let matches = Mutex::new(Vec::with_capacity(2));

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        matches
            .lock()
            .expect("matches lock should work")
            .extend(batch.into_iter().map(|matched| {
                (
                    matched.path.to_vec(),
                    matched.line_number,
                    matched.text.to_vec(),
                )
            }));
        true
    })
    .expect("streaming grep should succeed");

    assert_eq!(
        *matches.lock().expect("matches lock should work"),
        vec![
            (path.as_os_str().as_bytes().to_vec(), 1, b"needle".to_vec()),
            (
                path.as_os_str().as_bytes().to_vec(),
                3,
                b"needle twice needle".to_vec()
            ),
        ]
    );
}

#[test]
fn grep_stream_takes_the_serial_path_for_a_small_tree() {
    let directory = TempDir::new().expect("should create temp directory");
    for index in 0..4 {
        std::fs::write(
            directory.path().join(format!("f{index}.txt")),
            "needle\nother\n",
        )
        .expect("should write file");
    }
    let config = GrepConfig {
        pattern: "needle".to_owned(),
        search: SearchConfig {
            paths: vec![directory.path().to_path_buf()],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let batches = Mutex::new(Vec::with_capacity(1));

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        batches
            .lock()
            .expect("batches lock should work")
            .push(batch.len());
        true
    })
    .expect("streaming grep should succeed");

    assert_eq!(*batches.lock().expect("batches lock should work"), vec![4]);
}

#[test]
fn grep_stream_is_complete_when_the_pre_scan_overflows() {
    let directory = TempDir::new().expect("should create temp directory");
    let lines = 5_000;
    std::fs::write(directory.path().join("dense.txt"), "needle\n".repeat(lines))
        .expect("should write file");
    let config = GrepConfig {
        pattern: "needle".to_owned(),
        search: SearchConfig {
            paths: vec![directory.path().to_path_buf()],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let seen = Mutex::new(0_usize);

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        *seen.lock().expect("seen lock should work") += batch.len();
        true
    })
    .expect("streaming grep should succeed");

    // More matches than the pre-scan buffers, so it is discarded and replaced
    // by the parallel walk without losing or repeating a line.
    assert_eq!(*seen.lock().expect("seen lock should work"), lines);
}

#[test]
fn grep_stream_is_complete_when_the_entry_limit_flush_overflows() {
    let directory = TempDir::new().expect("should create temp directory");
    let matching = directory.path().join("matching.txt");
    let other = directory.path().join("other");
    let lines = 4_097;
    std::fs::write(&matching, "needle\n".repeat(lines)).expect("should write matching file");
    std::fs::create_dir(&other).expect("should create other directory");
    for index in 0..511 {
        std::fs::write(other.join(format!("f{index:04}.txt")), "other\n")
            .expect("should write other file");
    }
    let config = GrepConfig {
        pattern: "needle".to_owned(),
        search: SearchConfig {
            paths: vec![matching, other],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let seen = Mutex::new(0_usize);

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        *seen.lock().expect("seen lock should work") += batch.len();
        true
    })
    .expect("streaming grep should succeed");

    // The final two pre-scan matches flush only when the entry limit bails.
    assert_eq!(*seen.lock().expect("seen lock should work"), lines);
}

#[test]
fn grep_stream_is_complete_when_the_byte_limit_flush_overflows() {
    let directory = TempDir::new().expect("should create temp directory");
    let matching = directory.path().join("matching.txt");
    let oversized = directory.path().join("oversized.txt");
    let lines = 4_097;
    std::fs::write(&matching, "needle\n".repeat(lines)).expect("should write matching file");
    std::fs::File::create(&oversized)
        .expect("should create oversized file")
        .set_len(8 * 1024 * 1024)
        .expect("should size oversized file");
    let config = GrepConfig {
        pattern: "needle".to_owned(),
        search: SearchConfig {
            paths: vec![matching, oversized],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let seen = Mutex::new(0_usize);

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        *seen.lock().expect("seen lock should work") += batch.len();
        true
    })
    .expect("streaming grep should succeed");

    // The final two pre-scan matches flush only when the byte limit bails.
    assert_eq!(*seen.lock().expect("seen lock should work"), lines);
}

#[test]
fn grep_occurrence_stream_stops_after_the_first_match() {
    let directory = TempDir::new().expect("should create temp directory");
    let path = directory.path().join("dense.txt");
    std::fs::write(&path, "a".repeat(4_096)).expect("should write file");

    for byte_range in [false, true] {
        let config = GrepConfig {
            pattern: "a".to_owned(),
            format: if byte_range {
                GrepFormat::ByteRange
            } else {
                GrepFormat::Column
            },
            search: SearchConfig {
                paths: vec![path.clone()],
                ..SearchConfig::default()
            },
            ..GrepConfig::default()
        };
        let batches = Mutex::new(Vec::with_capacity(1));

        grep_stream(&config, &AtomicBool::new(false), |batch| {
            batches
                .lock()
                .expect("batches lock should work")
                .push(batch.len());
            false
        })
        .expect("stopping occurrence grep should succeed");

        assert_eq!(*batches.lock().expect("batches lock should work"), vec![1]);
    }
}

#[test]
fn grep_byte_range_indexes_returned_crlf_text() {
    let directory = TempDir::new().expect("should create temp directory");
    let path = directory.path().join("crlf.txt");
    std::fs::write(&path, b"foo\r\n").expect("should write file");
    let config = GrepConfig {
        pattern: r"\r".to_owned(),
        format: GrepFormat::ByteRange,
        search: SearchConfig {
            paths: vec![path],
            ..SearchConfig::default()
        },
        ..GrepConfig::default()
    };
    let results = Mutex::new(Vec::with_capacity(1));

    grep_stream(&config, &AtomicBool::new(false), |batch| {
        results
            .lock()
            .expect("results lock should work")
            .extend(batch);
        true
    })
    .expect("byte range grep should succeed");

    let results = results.into_inner().expect("results lock should work");
    let matched = results.first().expect("should find carriage return");
    assert_eq!(
        matched.position,
        Some(GrepPosition::ByteRange {
            offset: 3,
            length: 1,
        })
    );
    assert_eq!(&*matched.text, b"foo\r");
}
