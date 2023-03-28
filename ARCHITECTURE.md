# Architecture

(work in progress/original description; does not necessarily represent the
current state of the code)

## Definitions

Ground Control _specifications_ are composed of one or more _processes,_ each of
which run one or more _commands._ A specification is an ordered list of
processes to start when Ground Control is launched. Each process runs one or
more commands as part of its startup and shutdown behavior.

Processes can be grouped into two types: one-shot and daemon. Daemon processes
are characterized by the existence of a `run` command which is expected to run
in the foreground until the command is stopped by way of the process's `stop`
command or signal.

One-shot processes do _not_ have a `run` command, nor do they have a `stop`
command, since the process did not leave a persistent command running that needs
to be stopped.

All process types can have `pre` and `post` commands. `pre` is run _before_ the
`run` command, `post` is run _after_ the `stop` command. One-shot processes can
use `pre` and `post` commands, and in fact that is their primary function: to
execute short-running commands during the startup and shutdown of the Ground
Control system.

## Responsibilities

-   `Command` is all about executing commands and giving you a handle to the
    running command (which may terminate immediately, or may run for a while).
-   `Process` does the overall process management: `pre`, `run`, `stop`, `post`;
    and monitoring of the process's main (`run`) Command state, communicating
    that state upwards, _in aggregate,_ to Ground Control.
-   `GroundControl` runs processes, waits for any form of shutdown signal, and
    then stops everything.

## Execution

The [`Process`][] is a top-level construct that can be "started" and "stopped."
Processes are started in the order they are found in the Ground Control
Specification. Starting a process executes its `pre` and `run` commands (if
present), and then returns a `Process` object that can be used to stop the
process.

The _implementation_ of "stopped" varies by process type:

-   Daemon processes are considered "stopped" when their `run` command exits,
    either because it exited on its own or because the `stop` command was run
    and triggered the command to exit.
-   One-shot processes are considered "stopped" when [`Process.stop()`] is
    called. In other words, one-shot processes do _not_ have a `run` command,
    but do behave as if they had an non-exiting, _never-failing_ `run` command.
    One-shot processes will never trigger an abornmal shutdown; they can only be
    "stopped" by way of the `Process.stop()` function.

These subtle differences in implementation ensure that, at the Ground Control
level, all processes look the same: they run until stopped, regardless of if
they started a long-running daemon process.

During shutdown, regardless of it was triggered externally or due to the failure
of a daemon process, each Process is stopped, Ground Control waits for the
process to stop, and then the `post` command is run. This ensures that the
`post` command is run regardless of how the process was stopped (either due to
an early exit, or a controlled shutdown).

## Process Control and Monitoring

Processes (and to a lesser extent, Commands) have a unique problem, which is
that they need to both notify Ground Control of their exit status, _and_ allow
Ground Control to stop the process (which then permutes the exit status), which
then also requires the Process itself to know when it has exited. This means
that Ground Control needs to observe the state of the Process, but so does the
`stop` method of the Process.

This effectively requires mutability of the notification channel to be shared by
Ground Control and Process, but then also relay the completion status twice
(once to each listener). And yet Ground Control may not actually listen for the
first completion, because it only cares about _any_ Process exiting abnormally,
not _every_ Process exiting. The latter is only interesting during the shutdown
phase, where it is _Process_ that needs to ensure that the daemon exits.

Ground Control solves this problem with a fairly straightforward solution: the
Process itself is in charge of its own monitoring and, crucially, is the only
part of the system that can positively wait for confirmation of that specific
Process's shutdown (which it does in the `stop` function). Meanwhile, _Ground
Control_ asks every Process to notify an MPSC channel when the Process exits,
which it can then use to determine if any Process has exited (thus beginning the
shutdown sequence). The same MPSC channel can be used to initiate a graceful
shutdown, as Ground Control routes the signal handlers to the same MPSC channel.

This architecture reduces Process down to a single behavior: Processes can be
run, and Processes can be stopped. The exact state of a Process can not be
determined externally. Instead, Ground Control expects to be told when _any_
Process exits, which then begins the only time that Ground Control _does_ have
knowledge of a single Process's state: right after it has called that Process's
`stop` function and the function has returned.
