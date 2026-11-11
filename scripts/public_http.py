"""Small, bounded HTTPS client for public-source discovery.

Connections are pinned to validated public DNS answers, TLS is verified, and
redirects are restricted to the same named site. No cookies are retained.
"""
from datetime import datetime, timezone
import hashlib
import http.client
import ipaddress
import socket
import ssl
import threading
import time
from urllib.parse import urljoin, urlsplit

USER_AGENT = 'PanopticonNull/0.0.4 (+https://github.com/H4rmon1c/panopticon-null)'
_last = {}
_lock = threading.Lock()


class BudgetExhausted(ValueError):
    pass


def public_ip(raw):
    ip = ipaddress.ip_address(raw)
    if (not ip.is_global or ip.is_multicast or ip.is_loopback or ip.is_link_local
            or ip.is_unspecified or ip.is_reserved):
        return False
    if ip.version == 6:
        if ip.ipv4_mapped:
            return False
        for network in ('64:ff9b::/96', '64:ff9b:1::/48', '2001::/23', '2002::/16', '3fff::/20'):
            if ip in ipaddress.ip_network(network):
                return False
    return True


def checked_url(url, hosts):
    if len(url) > 4000 or any(ord(c) < 32 or ord(c) == 127 for c in url):
        raise ValueError('invalid bounded URL')
    value = urlsplit(url)
    if (value.scheme != 'https' or value.port not in (None, 443) or not value.hostname
            or value.hostname.lower() not in hosts or value.username or value.password
            or value.fragment or '\\' in value.path):
        raise ValueError('URL outside HTTPS host scope')
    return value


def wait_turn(host, interval):
    # Reserve the next slot while holding the lock so concurrent tasks never
    # accidentally send a burst to the same host.
    with _lock:
        now = time.monotonic()
        target = max(now, _last.get(host, now))
        _last[host] = target + interval
    if target > now:
        time.sleep(target - now)


def get(url, hosts, *, maximum=1_500_000, timeout=12, interval=1.0, redirects=2, redirect_guard=None, budget=None):
    """Return bounded raw bytes and a receipt for each validated request hop."""
    if not 0 < maximum <= 20 * 1024 * 1024 or not 0 <= redirects <= 3:
        raise ValueError('invalid request bounds')
    hosts = {host.lower() for host in hosts}
    observations = []
    for _ in range(redirects + 1):
        parsed = checked_url(url, hosts)
        host = parsed.hostname.lower()
        reserved = budget.reserve(maximum) if budget is not None else maximum
        if not reserved:
            raise BudgetExhausted('research job byte budget exhausted')
        wait_turn(host, interval)
        addresses = sorted({row[4][0] for row in socket.getaddrinfo(host, 443, type=socket.SOCK_STREAM)})
        if not addresses or any(not public_ip(ip) for ip in addresses):
            raise ValueError('empty, prohibited, or mixed DNS answers')
        last_error = None
        for address in addresses:
            conn = None
            raw = None
            try:
                started = time.monotonic()
                raw = socket.create_connection((address, 443), timeout=timeout)
                conn = ssl.create_default_context().wrap_socket(raw, server_hostname=host)
                raw = None
                conn.settimeout(timeout)
                path = parsed.path or '/'
                if parsed.query:
                    path += '?' + parsed.query
                request = f'GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: {USER_AGENT}\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n'
                conn.sendall(request.encode('ascii'))
                response = http.client.HTTPResponse(conn)
                response.begin()
                announced = response.getheader('Content-Length')
                if announced and (not announced.isdigit() or int(announced) > reserved):
                    raise ValueError('HTTP response exceeds byte limit')
                chunks = []
                size = 0
                while True:
                    remaining = timeout - (time.monotonic() - started)
                    if remaining <= 0:
                        raise TimeoutError('HTTP response deadline exceeded')
                    conn.settimeout(remaining)
                    chunk = response.read(min(65536, reserved + 1 - size))
                    if not chunk:
                        break
                    chunks.append(chunk)
                    size += len(chunk)
                    if size > reserved:
                        raise ValueError('HTTP response exceeds byte limit')
                data = b''.join(chunks)
                if budget is not None:
                    budget.release_unused(reserved - len(data))
                location = response.getheader('Location')
                receipt = {'url': url, 'retrieved_at': datetime.now(timezone.utc).isoformat(),
                           'status': response.status, 'resolved_ips': addresses, 'connected_ip': address,
                           'content_type': response.getheader('Content-Type', ''),
                           'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
                if location:
                    receipt['redirect_target'] = urljoin(url, location)
                observations.append((receipt, data))
                break
            except (OSError, ssl.SSLError, http.client.HTTPException) as error:
                last_error = error
            finally:
                if conn is not None:
                    conn.close()
                if raw is not None:
                    raw.close()
        else:
            raise RuntimeError('public HTTPS connection failed') from last_error
        if receipt['status'] not in (301, 302, 303, 307, 308):
            return observations
        if not receipt.get('redirect_target'):
            raise ValueError('redirect without destination')
        url = receipt['redirect_target']
        checked_url(url, hosts)
        if redirect_guard is not None and not redirect_guard(url):
            raise ValueError('redirect destination is disallowed by robots policy')
    raise ValueError('redirect limit exceeded')
