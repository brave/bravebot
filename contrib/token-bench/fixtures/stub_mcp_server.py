#!/usr/bin/env python3
"""A stand-in MCP server over stdio, standard library only.

It answers `initialize`, `tools/list` and `tools/call` with fixed tool schemas of the size a typical
server offers, so token-bench can measure what a set of MCP tools adds to every request without
depending on a real server. Each line on stdin is one JSON-RPC message and each answer is one
line on stdout.
"""

import json
import sys

PROTOCOL_VERSION = "2025-06-18"


def _tool(name, description, properties, required):
    return {
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {
                key: {"type": kind, "description": text} for key, (kind, text) in properties.items()
            },
            "required": required,
        },
    }


TOOLS = [
    _tool(
        "ledger_probe",
        "Look up one entry in the accounting ledger by its identifier and return the amount, the "
        "currency, the posting date and the account it was booked to. Use it when a question "
        "names a ledger entry. It never changes the ledger.",
        {"entry_id": ("string", "The ledger entry identifier, such as LE-10442."),
         "include_history": ("boolean", "Also return earlier revisions of the entry.")},
        ["entry_id"],
    ),
    _tool(
        "ledger_search",
        "Search ledger entries by account, date range or amount range and return up to the "
        "requested number of matches, newest first. Each match carries its identifier, amount and "
        "posting date.",
        {"account": ("string", "Account code to restrict the search to."),
         "from_date": ("string", "First posting date to include, as YYYY-MM-DD."),
         "to_date": ("string", "Last posting date to include, as YYYY-MM-DD."),
         "min_amount": ("number", "Smallest amount to include."),
         "max_amount": ("number", "Largest amount to include."),
         "limit": ("integer", "Most matches to return, at most 50.")},
        [],
    ),
    _tool(
        "invoice_status",
        "Return the state of an invoice: draft, sent, paid or overdue, with the dates each state "
        "began and the reminders that were sent.",
        {"invoice_number": ("string", "The invoice number printed on the document.")},
        ["invoice_number"],
    ),
    _tool(
        "customer_lookup",
        "Find a customer record by name or by email address and return the contact details, the "
        "billing country and the payment terms on file.",
        {"name": ("string", "Full or partial customer name."),
         "email": ("string", "Exact email address of the customer.")},
        [],
    ),
    _tool(
        "rate_table",
        "Return the exchange rate between two currencies on a given day from the treasury rate "
        "table. Rates are published once a day and are never interpolated.",
        {"base": ("string", "Three letter code of the currency to convert from."),
         "quote": ("string", "Three letter code of the currency to convert to."),
         "day": ("string", "The day to read the rate for, as YYYY-MM-DD.")},
        ["base", "quote"],
    ),
    _tool(
        "report_list",
        "List the scheduled reports with their names, owners and the time each one last ran.",
        {"owner": ("string", "Only list reports owned by this user."),
         "limit": ("integer", "Most reports to return.")},
        [],
    ),
    _tool(
        "report_fetch",
        "Fetch the latest output of a scheduled report as plain text, cut at the stated number of "
        "lines.",
        {"report": ("string", "The report name as report_list gives it."),
         "max_lines": ("integer", "Most lines to return.")},
        ["report"],
    ),
    _tool(
        "audit_trail",
        "Return the audit trail of a ledger entry or an invoice: who changed it, when, and the "
        "value before and after each change.",
        {"subject": ("string", "A ledger entry identifier or an invoice number."),
         "since": ("string", "Only changes after this timestamp, in RFC 3339 form.")},
        ["subject"],
    ),
]


def answer(request):
    method = request.get("method")
    if method == "initialize":
        return {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "bench-stub", "version": "1.0.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        name = (request.get("params") or {}).get("name")
        return {"content": [{"type": "text", "text": f"{name}: no data in the stand-in server"}],
                "isError": False}
    if method == "ping":
        return {}
    return None


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        if "id" not in request:
            continue
        result = answer(request)
        if result is None:
            reply = {"jsonrpc": "2.0", "id": request["id"],
                     "error": {"code": -32601, "message": "method not found"}}
        else:
            reply = {"jsonrpc": "2.0", "id": request["id"], "result": result}
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
