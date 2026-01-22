use crate::test::{colour_for_base58, Logger};
use log::{Level, Log, Record};
use radicle_term::{Color, Paint};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct TestWriter {
    data: Arc<Mutex<Vec<u8>>>,
}

impl Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.data.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_colour_for() {
    let s = "z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk";
    let color = colour_for_base58(s);
    assert_eq!(color, Color::Red);

    let s = "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi";
    let color = colour_for_base58(s);
    assert_eq!(color, Color::Blue);
}

#[test]
fn test_base58_ref_oid_regex_matching() {
    let logger = Logger::new(Level::Debug);

    let cases = vec![
        (
            "fetched rad:z42hL2jL4XNk6K8oHQaSWfMgCL7ji from z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            vec![
                "z42hL2jL4XNk6K8oHQaSWfMgCL7ji",
                "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            ],
        ),
        (
            "Setting ref: refs/rad/id -> 3143236b2e40338f5574ec04e935a5ab80a6868a",
            vec!["refs/rad/id", "3143236b2e40338f5574ec04e935a5ab80a6868a"],
        ),
        (
            "Syncing z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk with z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            vec![
                "z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
                "z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            ],
        ),
        (
            "Multiple refs: refs/heads/master and refs/tags/v1.0.0",
            vec!["refs/heads/master", "refs/tags/v1.0.0"],
        ),
        (
            "Mixed content: z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk and refs/remotes/origin/main",
            vec![
                "z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk",
                "refs/remotes/origin/main",
            ],
        ),
        (
            "No matches here",
            vec![],
        ),
        (
            "Short z123 is not matched",
            vec![],
        ),
        (
            "Timestamp 1769096403171 is matched by regex but filtered in log",
            vec!["1769096403171"],
        ),
    ];

    for (msg, expected) in cases {
        let matches: Vec<_> = logger
            .base58_ref_oid_re
            .find_iter(msg)
            .map(|m| m.as_str())
            .collect();
        assert_eq!(matches, expected, "Failed matching for input: '{}'", msg);
    }
}

#[test]
fn test_log_output() {
    Paint::force(true);

    let cases = vec![
        (
            "Hello z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk world",
            "test",
            vec![
                "\x1b[36mtest:\x1b[0m", // Target
                "\x1b[36mHello \x1b[0m", // Plain text (Cyan for test)
                "\x1b[1;31mz6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk\x1b[0m", // ID (Red + Bold)
                "\x1b[36m world\x1b[0m", // Plain text (Cyan)
            ],
        ),
        (
            "Syncing z6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk with z6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi",
            "test",
            vec![
                "\x1b[36mtest:\x1b[0m",
                "\x1b[36mSyncing \x1b[0m",
                "\x1b[1;31mz6Mkt67GdsW7715MEfRuP4pSZxJRJh6kj6Y48WRqVv4N1tRk\x1b[0m", // Red + Bold
                "\x1b[36m with \x1b[0m",
                "\x1b[1;34mz6MknSLrJoTcukLrE435hVNQT4JUhbvWLX4kUzqkEStBU8Vi\x1b[0m", // Blue + Bold
            ],
        ),
        (
            "Updated refs/heads/master",
            "sim",
            vec![
                "\x1b[1msim:\x1b[0m", // Target (Bold)
                "\x1b[1mUpdated \x1b[0m", // Plain text (Bold for sim)
                "\x1b[1;4mrefs/heads/master\x1b[0m", // Ref (Bold + Underline)
            ],
        ),
        (
            "No matches here",
            "test",
            vec![
                "\x1b[36mtest:\x1b[0m",
                "\x1b[36mNo matches here\x1b[0m",
            ],
        ),
        (
            "Timestamp 1769096403171 is matched as OID and given RGB colour",
            "test",
            vec![
                "\x1b[36mtest:\x1b[0m",
                "\x1b[36mTimestamp \x1b[0m",
                "\x1b[38;2;23;105;9m1769096403171\x1b[0m",
                "\x1b[36m is matched as OID and given RGB colour\x1b[0m",
            ]
        ),
        (
            "Commit 3143236b2e40338f5574ec04e935a5ab80a6868a",
            "test",
            vec![
                "\x1b[36mtest:\x1b[0m",
                "\x1b[36mCommit \x1b[0m",
                // OID painting: 314323 -> R=49, G=67, B=35.
                // No background.
                "\x1b[38;2;49;67;35m3143236b2e40338f5574ec04e935a5ab80a6868a\x1b[0m",
            ]
        ),
        (
            "Ref with ID refs/namespaces/z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z/refs/rad/sigrefs",
            "test",
            vec![
                "\x1b[36mRef with ID \x1b[0m",
                "\x1b[1;4mrefs/namespaces/\x1b[0m", // Prefix (Bold + Underline)
                // ID: z6Mkux...
                // We check for the ID string being present.
                "z6Mkux1aUQD2voWWukVb5nNUR7thrHveQG4pDQua8nVhib7Z",
                "\x1b[1;4m/refs/rad/sigrefs\x1b[0m", // Suffix (Bold + Underline)
            ]
        )
    ];

    for (msg, target, expected_parts) in cases {
        let data = Arc::new(Mutex::new(Vec::new()));
        let writer = TestWriter { data: data.clone() };
        let logger = Logger::with_writer(Level::Debug, Arc::new(Mutex::new(writer)));

        let args = format_args!("{}", msg);
        let record = Record::builder()
            .args(args)
            .level(Level::Info)
            .target(target)
            .build();

        logger.log(&record);

        let output = String::from_utf8(data.lock().unwrap().clone()).unwrap();

        for part in expected_parts {
            assert!(
                output.contains(part),
                "Output did not contain expected part: {:?}\nFull output: {:?}",
                part,
                output
            );
        }
    }

    Paint::force(false);
}
