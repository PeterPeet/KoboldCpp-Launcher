#!/usr/bin/env python3
"""Extract KoboldCpp's launch options from koboldcpp.py into a JSON table.

Reads the file with the `ast` module only; nothing from it is imported or executed.

    python3 -I tools/extract_args.py path/to/koboldcpp.py [version] > src-tauri/src/config/args.json

Each entry: dest, flags, group, exclusive (mutually exclusive group id or null), kind
(flag | value | list | optional), type (int | float | str | bool), default, const, choices,
min/max, hidden. Defaults computed at runtime (function calls) are recorded as null with
"dynamic": true.
"""
import ast
import json
import sys


def main():
    path = sys.argv[1]
    version = sys.argv[2] if len(sys.argv) > 2 else None
    tree = ast.parse(open(path, encoding="utf-8").read())

    # Module-level constants such as defaultport = 5001
    consts = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
            try:
                consts[node.targets[0].id] = ast.literal_eval(node.value)
            except Exception:
                pass

    def value(node):
        """Best-effort constant evaluation. Returns (value, ok)."""
        try:
            return ast.literal_eval(node), True
        except Exception:
            pass
        if isinstance(node, ast.Name) and node.id in consts:
            return consts[node.id], True
        if isinstance(node, ast.Attribute) and node.attr == "SUPPRESS":
            return "==SUPPRESS==", True
        if isinstance(node, (ast.List, ast.Tuple)):
            out = []
            for e in node.elts:
                v, ok = value(e)
                if not ok:
                    return None, False
                out.append(v)
            return out, True
        if isinstance(node, ast.Call) and getattr(node.func, "id", None) == "range" and all(isinstance(a, ast.Constant) for a in node.args):
            return list(range(*[a.value for a in node.args])), True
        return None, False

    groups = {}  # variable name -> (title, exclusive id)
    entries = []
    excl_counter = [0]

    def walk(body):
        for node in ast.walk(ast.Module(body=body, type_ignores=[])):
            if isinstance(node, ast.Assign) and isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Attribute):
                tgt = node.targets[0]
                fn = node.value.func
                if not isinstance(tgt, ast.Name):
                    continue
                parent = fn.value.id if isinstance(fn.value, ast.Name) else None
                ptitle, _ = groups.get(parent, ("General", None))
                if fn.attr == "add_argument_group":
                    title = node.value.args[0].value if node.value.args else "General"
                    groups[tgt.id] = (title, None)
                elif fn.attr == "add_mutually_exclusive_group":
                    excl_counter[0] += 1
                    groups[tgt.id] = (ptitle, f"{tgt.id}")

    # Find the __main__ block
    main_body = None
    for node in tree.body:
        if isinstance(node, ast.If) and "__main__" in ast.unparse(node.test):
            main_body = node.body
    if main_body is None:
        sys.exit("no __main__ block found")
    walk(main_body)

    for node in ast.walk(ast.Module(body=main_body, type_ignores=[])):
        if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "add_argument"):
            continue
        recv = node.func.value.id if isinstance(node.func.value, ast.Name) else "parser"
        group, exclusive = groups.get(recv, ("General", None))
        flags = [a.value for a in node.args if isinstance(a, ast.Constant)]
        kw = {k.arg: k.value for k in node.keywords}
        if "dest" in kw:
            dest = kw["dest"].value
        elif flags[0].startswith("-"):
            long = next((f for f in flags if f.startswith("--")), flags[0])
            dest = long.lstrip("-").replace("-", "_")
        else:
            dest = flags[0]
        e = {"dest": dest, "flags": flags, "group": group, "exclusive": exclusive}

        action = kw["action"].value if "action" in kw and isinstance(kw["action"], ast.Constant) else None
        nargs = kw["nargs"].value if "nargs" in kw and isinstance(kw["nargs"], ast.Constant) else None
        typ = "str"
        if "type" in kw:
            t = kw["type"]
            if isinstance(t, ast.Name):
                typ = t.id
            elif isinstance(t, ast.Call) and getattr(t.func, "id", None) == "check_range":
                typ = t.args[0].id
                e["min"], _ = value(t.args[1])
                e["max"], _ = value(t.args[2])
        if action == "store_true":
            e["kind"], typ = "flag", "bool"
        elif nargs in ("*", "+") or isinstance(nargs, int):
            e["kind"] = "list"
            e["nargs"] = nargs
        elif nargs == "?":
            e["kind"] = "optional"
        else:
            e["kind"] = "value"
        e["type"] = typ

        if "default" in kw:
            v, ok = value(kw["default"])
            e["default"] = v if ok else None
            if not ok:
                e["dynamic"] = True
        else:
            e["default"] = False if action == "store_true" else None
        if "const" in kw:
            e["const"], _ = value(kw["const"])
        if "choices" in kw:
            v, ok = value(kw["choices"])
            if ok:
                e["choices"] = v
        if "help" in kw:
            v, _ = value(kw["help"])
            e["hidden"] = v == "==SUPPRESS=="
            if isinstance(kw["help"], ast.Constant) and not e["hidden"]:
                e["help"] = kw["help"].value
            elif isinstance(kw["help"], ast.JoinedStr):
                e["help"] = ast.unparse(kw["help"])[2:-1]
        entries.append(e)

    json.dump({"source": "koboldcpp.py", "version": version, "args": entries}, sys.stdout, indent=1, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
