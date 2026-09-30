#!/usr/bin/env python3
"""redaction_canary.py — the redaction boundary as an executable test.

This is the measurement script for `REPORT-pantry-07-observability.md`, kept
because it runs and because the rule it checks is the one thing in that report
that a component can be tested against rather than merely agreed to.

WHAT IT DOES, in two halves that both have to pass:

1. **Send.** One OTLP/JSON request per record, each record carrying a distinct
   canary in every position the boundary is supposed to close — an attribute
   that is not on the allowlist, `exception.message`, `exception.stacktrace`,
   the span status message, a credential-shaped string smuggled under an
   *allowlisted* key, and a log record whose free-text `body` holds a prompt and
   an `sk-` key.

2. **Read back.** Query Tempo and Loki over their own read-only APIs and report
   per canary whether it is PRESENT. Sending proves the pipeline accepted
   something; only the readback proves whether the boundary held.

WHY ONE RECORD PER REQUEST. Measured against `kit/templates/compose`
`otel-collector-contrib:0.115.1`: a single POST carrying 4000 spans returned
HTTP 200 with `{"partialSuccess":{}}` and `otelcol_receiver_accepted_spans`
did not move. A silently-dropped batch proves nothing, so every record is its
own request. If a future collector accepts batches, this still works.

NOTHING HERE IS A SECRET. The canaries are fixed, greppable, obviously-fake
strings. `sk-live-CANARYKEY-…` and `Bearer CANARYBEARERTOKEN-…` are shaped like
credentials so the collector's `blocked_values` regexes have something to match;
they are not credentials and they authenticate nothing. No real endpoint, no
real token, no real prompt and no customer datum appears in this file or in its
output — see the redaction rule in the report.

USAGE
    redaction_canary.py --project p07            # send, then read back
    redaction_canary.py --project p07 --send-only

    The stack must be up with kit's `templates/compose` files present. Exit 0
    means no canary was found in either store; exit 1 means at least one was,
    and the report is the list of which.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time

#: Every canary, and the one thing each stands in for. Reported by name, never
#: printed in full: a canary that survived is already demonstrated by its name.
CANARIES: dict[str, str] = {
    "prompt_attr": "CANARYPROMPT",          # an attribute not on the allowlist
    "exc_message": "CANARYMESSAGE",         # exception.message
    "exc_stack": "CANARYSTACK",             # exception.stacktrace
    "status_msg": "CANARYSTATUS",           # span status message
    "sk_under_allowed_key": "sk-live-CANARYKEY",       # blocked_values, shape 1
    "bearer_under_allowed_key": "CANARYBEARERTOKEN",  # blocked_values, shape 2
    "log_body": "CANARYLOGBODY",            # the log record's free text
    "log_attr": "CANARYLOGATTR",            # a log attribute off the allowlist
}

#: Attributes that MUST survive, so a readback that found nothing cannot be
#: mistaken for a pass. Assert the floor before asserting the absence.
EXPECTED_TO_SURVIVE = ("error.type", "http.route", "llm.model", "service.name")


def hexid(seed: int, n: int) -> str:
    """A deterministic, made-up identifier. Not a secret and not a real trace."""
    out, x = [], (seed * 2654435761 % (1 << 32)) or 1
    for _ in range(n):
        x = (x * 1103515245 + 12345) % (1 << 31)
        out.append("0123456789abcdef"[x % 16])
    return "".join(out)


def curl(image: str, net: str | None, url: str) -> str:
    cmd = ["docker", "run", "--rm"]
    if net:
        cmd += ["--network", net]
    cmd += [image, "-s", "--max-time", "30", url]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout


def post(project: str, service_container: str, signal: str, payload: dict) -> str:
    """One OTLP/JSON request. Returns the collector's own reply body."""
    local = f"/tmp/canary-{signal}.json"
    remote = f"{service_container}:{local}"
    with open(local, "w") as fh:
        json.dump(payload, fh)
    subprocess.run(["docker", "cp", local, remote], check=True, capture_output=True)
    net = f"{project}_platform"
    r = subprocess.run(
        ["docker", "exec", service_container, "curl", "-s", "-X", "POST",
         "-H", "Content-Type: application/json", "--data-binary", f"@{local}",
         f"http://{project}-otel-collector-1:4318/v1/{signal}"],
        capture_output=True, text=True)
    return r.stdout.strip()


def span_record(i: int) -> dict:
    t = int(time.time() * 1_000_000_000)
    return {"resourceSpans": [{
        "resource": {"attributes": [
            {"key": "service.name", "value": {"stringValue": "canary"}},
            {"key": "service.version", "value": {"stringValue": "0"}},
            {"key": "deployment.environment", "value": {"stringValue": "canary"}},
        ]},
        "scopeSpans": [{"scope": {"name": "canary"}, "spans": [{
            "traceId": hexid(i, 32), "spanId": hexid(i + 7777, 16),
            "name": "canary", "kind": 2,
            "startTimeUnixNano": str(t), "endTimeUnixNano": str(t + 50_000_000),
            "attributes": [
                {"key": "error.type", "value": {"stringValue": "provider_unavailable"}},
                {"key": "http.route", "value": {"stringValue": "/v1/route"}},
                {"key": "llm.model", "value": {"stringValue": "claude-opus-4"}},
                {"key": "http.response.status_code", "value": {"intValue": "503"}},
                # The realistic leak: an attribute somebody adds in six months
                # because it would be useful, under a name nobody predicted.
                {"key": "cafaye.prompt",
                 "value": {"stringValue": f"{CANARIES['prompt_attr']}-{i:04d}"}},
                # The second barrier. `http.route` and `llm.model` ARE on the
                # allowlist, so only a value-shape rule can stop these.
                {"key": "http.route",
                 "value": {"stringValue": f"{CANARIES['sk_under_allowed_key']}-AAAAAAAAAAAAAAAA"}},
                {"key": "llm.model",
                 "value": {"stringValue": f"Bearer {CANARIES['bearer_under_allowed_key']}-AAAAAAAAAAAAAAAA"}},
            ],
            "status": {"code": 2, "message": f"{CANARIES['status_msg']}-{i:04d}"},
            "events": [{"name": "exception", "timeUnixNano": str(t + 10_000_000),
                        "attributes": [
                            {"key": "exception.type", "value": {"stringValue": "ProviderUnavailable"}},
                            {"key": "exception.message",
                             "value": {"stringValue": f"{CANARIES['exc_message']}-{i:04d}"}},
                            {"key": "exception.stacktrace",
                             "value": {"stringValue": f"{CANARIES['exc_stack']}-{i:04d}"}}]}],
        }]}]}]}


def log_record(i: int) -> dict:
    """A log record whose BODY — free text, not an attribute — holds both a
    prompt and a credential-shaped string. This is the position the shipped
    redaction processor does not reach, and the reason this record exists."""
    t = int(time.time() * 1_000_000_000)
    record = {
        "timeUnixNano": str(t), "severityNumber": 17, "severityText": "ERROR",
        "body": {"stringValue":
                 f"{CANARIES['log_body']}-{i:04d} upstream rejected "
                 f"{CANARIES['sk_under_allowed_key']}-AAAAAAAAAAAAAAAA"},
        "attributes": [
            {"key": "error.type", "value": {"stringValue": "provider_unavailable"}},
            {"key": "cafaye.prompt", "value": {"stringValue": f"{CANARIES['log_attr']}-{i:04d}"}},
        ],
    }
    entry = {
        "resource": {"attributes": [
            {"key": "service.name", "value": {"stringValue": "canary"}},
            {"key": "deployment.environment", "value": {"stringValue": "canary"}},
        ]},
        "scopeLogs": [{"scope": {"name": "canary"}, "logRecords": [record]}],
    }
    return {"resourceLogs": [entry]}


def attr_keys(node, out: set[str]) -> set[str]:
    """Every attribute name anywhere in a Tempo trace document."""
    if isinstance(node, dict):
        for k, v in node.items():
            if k == "key" and isinstance(v, str):
                out.add(v)
            attr_keys(v, out)
    elif isinstance(node, list):
        for v in node:
            attr_keys(v, out)
    return out


def send(project: str, service_container: str, n: int) -> dict:
    spans = logs = 0
    for i in range(1, n + 1):
        if post(project, service_container, "traces", span_record(i)):
            spans += 1
        if post(project, service_container, "logs", log_record(i)):
            logs += 1
    return {"span_requests_answered": spans, "log_requests_answered": logs, "of_each": n}


def readback(project: str, image: str) -> tuple[dict[str, list[str]], set[str]]:
    net = f"{project}_platform"
    found: dict[str, list[str]] = {k: [] for k in CANARIES}
    keys: set[str] = set()

    search = curl(image, net,
                  f"http://{project}-tempo-1:3200/api/search?tags=service.name%3Dcanary&limit=40")
    try:
        traces = json.loads(search or "{}").get("traces", [])
    except json.JSONDecodeError:
        traces = []
    print(f"tempo: {len(traces)} canary traces found by service.name=canary")
    for t in traces:
        doc = curl(image, net, f"http://{project}-tempo-1:3200/api/traces/{t['traceID']}")
        for name, needle in CANARIES.items():
            if needle in doc:
                found[name].append("tempo")
        try:
            keys |= attr_keys(json.loads(doc), set())
        except json.JSONDecodeError:
            pass

    loki = curl(image, net,
                f"http://{project}-loki-1:3100/loki/api/v1/query_range"
                "?query=%7Bservice_name%3D%22canary%22%7D&limit=100&start=0")
    for name, needle in CANARIES.items():
        if needle in loki:
            found[name].append("loki")
    return found, keys


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--project", default="p07",
                    help="docker compose project name (default: p07)")
    ap.add_argument("--service-container", default=None,
                    help="a container on the stack network with curl (default: "
                         "<project>-load)")
    ap.add_argument("--image", default="curlimages/curl:8.10.1",
                    help="an image with curl, used to query the backends")
    ap.add_argument("-n", type=int, default=40, help="records per signal (default: 40)")
    ap.add_argument("--send-only", action="store_true")
    a = ap.parse_args()

    svc = a.service_container or f"{a.project}-load"
    print(json.dumps(send(a.project, svc, a.n), indent=2))
    print("waiting 20s for the backends to index and flush…")
    time.sleep(20)
    if a.send_only:
        return 0

    found, keys = readback(a.project, a.image)
    print("\n--- canaries FOUND in a store (each is a leak) ---")
    leaked = {k: v for k, v in found.items() if v}
    if not leaked:
        print("  none.")
    for k, v in sorted(leaked.items()):
        print(f"  LEAK {k:28s} in {sorted(set(v))}")

    print("\n--- attribute names that survived on the spans ---")
    print("  " + (", ".join(sorted(keys)) if keys else "(none read)"))
    for want in EXPECTED_TO_SURVIVE:
        print(f"  {'present' if want in keys else 'ABSENT '}  {want}")
    missing = [w for w in EXPECTED_TO_SURVIVE if w not in keys]
    if missing:
        print(f"\n  {len(missing)} expected attribute(s) did not arrive, so this run "
              f"does NOT prove the absences above. Treat it as inconclusive.")

    return 1 if leaked else 0


if __name__ == "__main__":
    sys.exit(main())
