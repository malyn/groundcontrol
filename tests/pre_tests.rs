//! Tests that verify different aspects of the `pre` scripts that run as
//! part of starting daemons and, in the case of "one-shot" processes,
//! are the only thing that does run during the startup phase.

use crate::common::{start, stop};

mod common;

/// Verifies that a failed `pre` execution aborts all subsequent command
/// executions *and* runs stop/post commands for anything that was
/// started.
#[test_log::test(tokio::test)]
async fn failed_pre_aborts_startup() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = "/user/binary/nope"
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;
    assert_eq!(Err(groundcontrol::Error::StartupAborted), result);
    assert_eq!("a-pre\na-post\n", output);
}

/// Verifies that a failed `pre` execution shuts down all
/// previously-started long-running processes.
///
/// Note that we cannot have the test daemon write to the normal result
/// path, because Ground Control doesn't actually wait for the daemon to
/// finish starting (it just makes sure that it starts running at all)
/// and so *sometimes* we might get the daemon's `run` output appended
/// to the file, and sometimes not.
#[test_log::test(tokio::test)]
async fn failed_pre_shuts_down_earlier_processes() {
    let config = r##"
        [[processes]]
        name = "a"
        pre = [ "/bin/sh", "-c", "echo a-pre >> {result_path}" ]
        run = [ "/bin/sh", "{test-daemon.sh}", "a-daemon", "{result_path}-alternate", "{temp_path}" ]
        post = [ "/bin/sh", "-c", "echo a-post >> {result_path}" ]

        [[processes]]
        name = "b"
        pre = "/user/binary/nope"
        post = [ "/bin/sh", "-c", "echo b-post >> {result_path}" ]

        [[processes]]
        name = "c"
        pre = [ "/bin/sh", "-c", "echo c-pre >> {result_path}" ]
        post = [ "/bin/sh", "-c", "echo c-post >> {result_path}" ]
        "##;

    let (gc, _tx, dir) = start(config).await;
    let (result, output) = stop(gc, dir).await;
    assert_eq!(Err(groundcontrol::Error::StartupAborted), result);
    assert_eq!("a-pre\na-post\n", output);
}
