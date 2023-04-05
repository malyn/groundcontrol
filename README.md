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

See [this blog post][groundcontrolpost] for more information on the
genesis of Ground Control.

[foreman]: https://ddollar.github.io/foreman/
[groundcontrolpost]: https://michaelalynmiller.com/blog/2023/04/05/multi-process-docker-containers/
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
