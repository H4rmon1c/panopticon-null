# Private page-OCR backlog prototype

This serial queue wraps the separately guarded `private-research-page.py`
worker. It never exports research rows, changes coverage metrics, publishes
matches, schedules itself, or treats an OCR result as complete coverage. It
requires the worker's per-page systemd/cgroup/tmpfs isolation to be installed
and independently verified first; this patch does not provide that worker.

Place a queue file within a private owner-only root. Each job names a child
archive created by the existing receipt-bound private screener and an explicit
page from 1 to 300. Page numbers must come from a separately verified page
inventory; this prototype does not infer page count from a successful first
page, especially when blank pages fail OCR.

```json
{"jobs":[{"archive":"archive-a","page":1},{"archive":"archive-b","page":3}]}
```

Example invocation after installing the audited worker:

```sh
python3 scripts/private-ocr-backlog.py --private-root /absolute/private/root \
  --queue /absolute/private/root/queue.json --binary /absolute/path/to/pnull \
  --tessdata /absolute/path/to/tessdata --max-pages 10 --max-seconds 1200
```

The run is serial and attempts at most 50 pages. It reserves 105 seconds for
each worker call and checks its run deadline before starting one. Filesystem
reads, checkpoint writes, and operating-system scheduling add overhead, so the
deadline is not a strict end-to-end wall-clock guarantee. Errors remain
incomplete and can be retried; never-attempted pages take priority so a failing
page does not starve the queue. Failed pages are retried oldest attempt first.
Before touching queue state, the scheduler checks that the executable and
English language-data file are present, bounded regular files. This reports a
missing dependency rather than treating every queued page as incomplete.
State is atomically saved under the private
root and bound to the exact queue bytes. Only one run may hold its lock.
The queue, checkpoint, lock and cached-page reads use a pinned private-root
descriptor and reject symlinks in every path component. The queue passes pinned
root and archive descriptors to the worker, which verifies their identities and
reads only through the pinned archive. Checkpoints bind cached artifacts to the
archive's device/inode as well as the artifact hash. Older entries without that
identity are reprocessed.

Success records mean only that the worker reported a provisional page result.
On later runs an unchanged artifact is skipped as `cached_not_reverified`;
the private checkpoint and its hash are scheduling aids, not authenticated
evidence. Run the page worker directly to independently re-extract and compare
an existing artifact. Any missing, changed, failed, blank, timed-out, or
unattempted page remains incomplete or deferred. A queue cannot establish
that an entire document or jurisdiction was searched.
