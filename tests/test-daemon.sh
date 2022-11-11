#!/usr/bin/env sh
set -Eeuo

DAEMON_NAME=$1
RESULT_PATH=$2
PID_BASE_PATH=$3

# Write our startup flag to the result file.
echo ${DAEMON_NAME}:started >> ${RESULT_PATH}

# Start a five second background sleep and capture the PID.
sleep 5 &
SLEEP_PID=$!

# Trap the SIGTERM shutdown signal (which kills the sleep process).
function do_shutdown() {
    echo ${DAEMON_NAME}:shutdown-requested >> ${RESULT_PATH}
    kill ${SLEEP_PID}
}
trap 'do_shutdown' TERM

# Write out the PID of the *this* process, which the test code can use
# to determine that the daemon has started, and to stop the daemon.
echo $$ > ${PID_BASE_PATH}/${DAEMON_NAME}.pid

# Wait for sleep to exit/be stopped, and ignore its exit code (otherwise
# the test-daemon script will fail).
wait ${SLEEP_PID} || true

# Clean exit.
echo ${DAEMON_NAME}:stopped >> ${RESULT_PATH}
exit 0