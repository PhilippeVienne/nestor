#!/usr/bin/env python3
"""Serveur MCP minimal (transport stdio) pour tester la passerelle de nestord.

Trois outils sur un carnet de notes tenu dans le fichier `NOTES_FILE` :
- `lire_notes`    : lecture, annoncee en lecture seule ;
- `ajouter_note`  : ecriture ;
- `tout_effacer`  : ecriture destructrice.
"""
import json
import os
import sys

NOTES_FILE = os.environ.get("NOTES_FILE", "/tmp/nestor-notes-test.txt")

TOOLS = [
    {
        "name": "lire_notes",
        "description": "Liste les notes du carnet.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"readOnlyHint": True},
    },
    {
        "name": "ajouter_note",
        "description": "Ajoute une note au carnet.",
        "inputSchema": {"type": "object", "properties": {"texte": {"type": "string"}}, "required": ["texte"]},
    },
    {
        "name": "tout_effacer",
        "description": "Efface tout le carnet.",
        "inputSchema": {"type": "object", "properties": {}},
        "annotations": {"destructiveHint": True},
    },
]


def read_notes():
    try:
        with open(NOTES_FILE, encoding="utf-8") as f:
            return [line.rstrip("\n") for line in f if line.strip()]
    except FileNotFoundError:
        return []


def call(name, arguments):
    if name == "lire_notes":
        notes = read_notes()
        return "\n".join(notes) if notes else "(carnet vide)"
    if name == "ajouter_note":
        with open(NOTES_FILE, "a", encoding="utf-8") as f:
            f.write(str(arguments.get("texte", "")) + "\n")
        return "note ajoutee"
    if name == "tout_effacer":
        open(NOTES_FILE, "w", encoding="utf-8").close()
        return "carnet efface"
    raise ValueError(f"outil inconnu : {name}")


for line in sys.stdin:
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        continue
    if "id" not in message:
        continue  # notification
    method = message.get("method")
    reply = {"jsonrpc": "2.0", "id": message["id"]}
    try:
        if method == "initialize":
            reply["result"] = {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "notes-test", "version": "0"},
            }
        elif method == "tools/list":
            reply["result"] = {"tools": TOOLS}
        elif method == "tools/call":
            params = message.get("params", {})
            text = call(params.get("name"), params.get("arguments") or {})
            reply["result"] = {"content": [{"type": "text", "text": text}]}
        else:
            reply["error"] = {"code": -32601, "message": f"methode inconnue : {method}"}
    except Exception as err:  # noqa: BLE001
        reply["error"] = {"code": -32000, "message": str(err)}
    sys.stdout.write(json.dumps(reply) + "\n")
    sys.stdout.flush()
