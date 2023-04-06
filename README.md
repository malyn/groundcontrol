# Ground Control

_Launch your services into the cloud!_

Ground Control is a process manager that **makes it easy to start multiple,
_dependent_ processes in micro-VMs or containers.** Ground Control offers
slightly more flexibility than [foreman][], with just enough of [systemd's][]
configurability to make it easy to express complicated startup and shutdown
processes.

Like [s6-overlay][], Ground Control was designed from the start for
container-based applications. Unlike s6-overlay, Ground Control doesn't need to
be PID 1 and is fully compatible with [micro-VM environments like Fly.io][].
(the need to get multi-process Docker containers running on Fly.io was the
impetus for Ground Control)

See [this blog post][groundcontrolpost] for more information on the genesis of
Ground Control.

[foreman]: https://ddollar.github.io/foreman/
[groundcontrolpost]:
    https://michaelalynmiller.com/blog/2023/04/05/multi-process-docker-containers/
[micro-vm environments like fly.io]: https://fly.io/blog/docker-without-docker/
[s6-overlay]: https://github.com/just-containers/s6-overlay
[systemd's]: https://systemd.io

## Features

-   Starts and monitors multiple processes using a simple, TOML-based config
    file.
-   Supports both one-shot and long-running processes.
-   Pre- and Post-startup _and_ shutdown hooks for all process types.
-   Environment variable filtering and routing: full control over which
    variables can be seen by each process.
-   Console output multiplexing; stdout/stderr from all processes will appear on
    Ground Control's output.
-   Basic dependency management through predictable startup and shutdown
    ordering.
-   No external dependencies. Does not rely on the presence of a shell to start
    or stop processes, or to pass environment variables as arguments to
    commands.

## Usage

Ground Control is provided as a Docker image containing the `groundcontrol`
binary. The `groundcontrol` binary takes a single argument, the path to a
`groundcontrol.toml` file.

Inclusion in your `Dockerfile` usually looks something like this:

```dockerfile
### Ground Control
FROM ghcr.io/malyn/groundcontrol AS groundcontrol

### Final Image
FROM openresty/openresty:1.21.4.1-6-alpine-apk

# Copy binaries, scripts, and config.
WORKDIR /app

COPY --from=groundcontrol /groundcontrol ./
COPY groundcontrol.toml ./
COPY nginx.conf /etc/nginx/conf.d/default.conf

ENTRYPOINT ["/app/groundcontrol", "/app/groundcontrol.toml"]
```

### groundcontrol.toml

All configuration is provided in the `groundcontrol.toml` file (also called the
"Ground Control specification"), which consists of an [array of
tables][tomltablearray] specifying the _processes_ that Ground Control will
manage.

For example:

```toml
[[processes]]
name = "hello"
pre = "/bin/echo Hello {{USER}}! How are you today?"

[[processes]]
name = "nginx"
run = [ "/usr/sbin/nginx", "-g", "daemon off;" ]
```

The above Ground Control specification consists of two processes: a one-shot
process that runs the `echo` commands and then exits, and a long-running process
that starts the NGINX web server. The "nginx" process will _not_ start until the
"hello" process has finished running (and if `/bin/echo` is not found, or exits
with a non-zero exit code, then NGINX will not be started and Ground Control
will also exit with a non-zero exit code).

[tomltablearray]: https://toml.io/en/v1.0.0#array-of-tables

#### Processes

Processes are started in order, and each process must start successfully before
the next process will be started. During shutdown, processes are stopped in the
reverse order. Shutdown can be initiated by a signal (`SIGINT` or `SIGTERM`),
and will be automatically initiated if any long-running process exits.

Processes consist of a name and zero or more _commands._ Commands are the
binaries or shell scripts that are used to start and stop the process.

#### Commands

Ground Control supports four types of commands (all of which are optional):

-   `pre`: One-shot command that runs as part of the startup phase.
-   `run`: Optional command that starts the long-running portion of this
    process. If not present, and `pre` _is_ present, then this process is
    considered a one-shot process. Note that all commands are optional, which
    means that a process could include only a `post` command if it's only
    purpose is to run a command during shutdown.
-   `stop`: Mechanism used to stop a long-running process: can be either a
    command (binary or shell script) or the name of a signal (`SIGINT`,
    `SIGQUIT`, or `SIGTERM`). Defaults to using `SIGTERM` to stop the command
    started by `run`. Ignored if the process does not include a `run` statement
    (since one-shot processes do not need to be "stopped").
-   `post`: Command to run during the shutdown phase, perhaps to clean up any
    resources used by the process, disconnect from a VPN, initiate a backup
    operation, etc. Both one-shot and long-running processes can use the `post`
    command.

Command values can take one of three formats (all of which can use the
environment variable expansion feature explained later):

-   A basic [TOML string][tomlstring]:

    ```toml
    [[processes]]
    name = "hello"
    pre = "/bin/echo -n Hello {{USER}}! How are you today?"
    ```

-   A [TOML array][tomlarray], where each array element is one argument to the
    process (this is helpful to avoid the need to quote special characters):

    ```toml
    [[processes]]
    name = "hello"
    pre = [ "/bin/echo", "-n", "Hello {{USER}}! How are you today?" ]
    ```

-   A [TOML table][tomltable], usually an [inline table][tomlinlinetable], used
    to set `user` or limit access to environment variables:

    ```toml
    [[processes]]
    name = "hello"
    pre = { user = "nobody", only-env = [], command = "/bin/echo -n Hello {{USER}}! How are you today?" }
    ```

    Tables can also use the expanded form (this example is equivalent to the one
    above):

    ```toml
    [[processes]]
    name = "hello"
    pre.user = "nobody"
    pre.only-env = []
    pre.command = [ "/bin/echo", "-n", "Hello {{USER}}! How are you today?" ]
    ```

    Note that the `command` can be either a plain string or an array.

[tomlarray]: https://toml.io/en/v1.0.0#array
[tomlinlinetable]: https://toml.io/en/v1.0.0#inline-table
[tomlstring]: https://toml.io/en/v1.0.0#string
[tomltable]: https://toml.io/en/v1.0.0#table

#### Environment Variables

Ground Control supports two features related to environment variables:
environment variable expansion, and environment variable filtering.

The former is used to pass an environment variable as the argument to a process,
and is required because Ground Control does _not_ execute in the context of a
shell, so there is no shell expansion available in Ground Control's command
values. Environment variable expansion is performed anywhere a command is found
-- command strings, arrays, or tables -- and uses a Mustache-style syntax:
`{{ VARNAME }}`

Environment variable filtering defaults to disabled, but can be enabled on a
_command-by-command_ basis. This can be used to limit the visibility of, for
example, auth tokens, database secrets, etc. to only those commands that need
those values. Filtering is enabled by setting the `only-env` value of a command
to the list of variables that should be available to the command. An empty array
is also valid, and means that _no_ environment variables will be available to
the process (except `PATH`, which is always included).

Examples:

-   The following command has access to every environment variable (because it
    does _not_ include an `only-env` statement):

    ```toml
    [[processes]]
    name = "printenv"
    pre = "/usr/bin/printenv"
    ```

-   This command is only able to see `USER` and `HOME` (and `PATH`, which Ground
    Control always includes in the environment):

    ```toml
    [[processes]]
    name = "printenv"
    pre = { only-env = ["USER", "HOME"], command = "/usr/bin/printenv" }
    ```

-   This command cannot see _any_ environment variables (other than `PATH`), but
    note that _environment variable expansion bypasses environment filtering,_
    so `USER` does not need to be included in order for expansion to work:

    ```toml
    [[processes]]
    name = "hello"
    pre = { only-env = [], command = "/bin/echo -n Hello {{USER}}! How are you today?" }
    ```

-   Different commands in the same process can have access to different
    environment variables:

    ```toml
    [[processes]]
    name = "database-server"
    pre = { only-env = ["DB_PASSWORD"], command = "/restore/database/from/cloud /data/db" }
    run = { only-env = [], command = "/db/server /data/db" }

    [[processes]]
    name = "web-server"
    run = { only-env = ["OAUTH_SECRET"], command = "/my/web/service" }
    ```

    In this example, the "database-server" `pre` command has access to the
    database password, but the database server itself (which uses the restored
    database) cannot see the password. The "web-server" process cannot see the
    `DB_PASSWORD`, but _can_ see the `OAUTH_SECRET`.

## Examples

-   [Super Guppy][superguppy] uses Ground Control to provide a
    batteries-included private crate registry for Rust projects. (Super Guppy's
    [`groundcontrol.toml`][superguppygctoml] file)
-   [dialtun][dialtun] dynamically maps HTTP ports on dev boxes to public HTTPS
    hostnames, and uses Ground Control to start Tailscale before running an
    NGINX server. (dialtun's [`groundcontrol.toml`][dialtungctoml] file)

[dialtun]: https://github.com/malyn/dialtun
[dialtungctoml]: https://github.com/malyn/dialtun/blob/main/groundcontrol.toml
[superguppy]: https://github.com/malyn/superguppy
[superguppygctoml]:
    https://github.com/malyn/superguppy/blob/main/groundcontrol.toml

## Conduct

This project adheres to the
[Contributor Covenant Code of Conduct](https://github.com/malyn/groundcontrol/blob/main/CODE_OF_CONDUCT.md).
This describes the minimum behavior expected from all contributors.

## License

Licensed under either of

-   Apache License, Version 2.0
    ([LICENSE-APACHE](https://github.com/malyn/groundcontrol/blob/main/LICENSE-APACHE)
    or <https://www.apache.org/licenses/LICENSE-2.0>)
-   MIT license
    ([LICENSE-MIT](https://github.com/malyn/groundcontrol/blob/main/LICENSE-MIT)
    or <https://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
