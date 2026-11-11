#!/usr/bin/env bash
# Install reviewed units/configuration on a provisioned host; never enable a timer.
set -euo pipefail
[[ "$(id -u)" == 0 ]] || { echo 'run as root on the VPS' >&2; exit 1; }
repo=/srv/panopticon/repo
[[ "$(realpath -e "$(dirname "$0")/../..")" == "$repo" && ! -L "$repo" ]] || {
    echo 'install the reviewed checkout at /srv/panopticon/repo first' >&2; exit 1;
}
[[ -z "$(find "$repo" -xdev \( ! -user root -o -perm /022 \) -print -quit)" ]] || {
    echo 'repository must be root-owned and not group/world writable' >&2; exit 1;
}
for command in python3 node caddy systemctl systemd-analyze flock realpath; do
    command -v "$command" >/dev/null || { echo "missing dependency: $command" >&2; exit 1; }
done
node -e 'if(Number(process.versions.node.split(".")[0])<22)process.exit(1)'
[[ -x /usr/local/bin/pnull && ! -L /usr/local/bin/pnull && "$(stat -c %U /usr/local/bin/pnull)" == root && -z "$(find /usr/local/bin/pnull -perm /022 -print)" ]] || { echo 'install the verified Rust release binary first' >&2; exit 1; }
getent passwd panopticon >/dev/null || useradd --system --home /var/lib/panopticon --shell /usr/sbin/nologin panopticon
# Open every directory component without following links; mutate only opened FDs.
# This stays safe on reruns when the service account owns the site directory.
python3 "$repo/deploy/selfhost/install-directories.py"
[[ ! -L /etc/panopticon/panopticon.env && ! -L /etc/caddy/Caddyfile.panopticon ]] || { echo 'configuration destination is a symlink' >&2; exit 1; }
if [[ ! -e /etc/panopticon/panopticon.env ]]; then
    install -o root -g panopticon -m 0640 "$repo/deploy/selfhost/panopticon.env.example" /etc/panopticon/panopticon.env
fi
for unit in "$repo"/deploy/selfhost/panopticon-*.service "$repo"/deploy/selfhost/panopticon-*.timer; do
    install -o root -g root -m 0644 "$unit" /etc/systemd/system/
done
install -o root -g root -m 0644 "$repo/web/panopticon-terminal/deploy/Caddyfile.example" /etc/caddy/Caddyfile.panopticon
caddy validate --config /etc/caddy/Caddyfile.panopticon --adapter caddyfile
systemctl daemon-reload
printf '%s\n' 'Units and proposed Caddy config installed. Restore private state, run preflight and sandbox test, then publish and activate timers explicitly.'
