# Compose V1 PTY Reproduction

This record captures the byte-level reproduction used to diagnose standalone Compose progress. It uses Docker Compose V1.29.2 in the shared testbox and does not start application services.

## Fixture

```yaml
services:
  pullprobe:
    image: debian:bookworm-slim
```

## Piped output

The command was executed over SSH, so the outer process had no terminal:

```sh
COMPOSE_PROGRESS=tty COMPOSE_ANSI=always docker run --rm \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v "$PWD":/work \
  docker/compose:1.29.2 \
  -f /work/compose.yml -p dockrev_v1probe pull \
  >piped.stdout 2>piped.stderr
```

The Compose result had `stdout=0 bytes`, `stderr=2533 bytes`, and no `ESC` byte in either stream. Its stderr tail contained successive text updates such as `downloading (26.0%)`, which is necessarily appended by a terminal consumer because there are no cursor movement controls.

## PTY output

The same fixture was run through util-linux `script`, keeping the process output captured by the outer shell:

```sh
script -q -f -c "docker run --rm -t \
  -e COMPOSE_PROGRESS=tty -e COMPOSE_ANSI=always \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v \"$PWD\":/work docker/compose:1.29.2 \
  -f /work/compose.yml -p dockrev_v1probe_pty pull" /dev/null \
  >pty.combined 2>pty.stderr
```

The result had `pty.combined=2559 bytes`, `pty.stderr=0 bytes`, an `ESC` byte, and 47 `CSI 2A` cursor-up sequences. The raw prefix included `\r\r\r\n\x1b[1A\x1b[2K\rPulling`. These controls make Docker layer progress overwrite the terminal screen rows instead of adding a row for every update.

The runtime wrapper adds `-e` to propagate the child status. On the same testbox, `script -q -f -e -c "exit 7" /dev/null` returned exit code 7.

Dockrev applies this PTY route only to standalone `docker-compose` pull streaming. The internal routing flag is removed before the child starts, and the published runtime image provides `script` through `util-linux`.
