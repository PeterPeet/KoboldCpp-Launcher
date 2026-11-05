#!/usr/bin/env python3
"""Extract the model architectures KoboldCpp's llama.cpp can load (LLM_ARCH_NAMES).

Reads the text of src/llama-arch.cpp from the KoboldCpp repo at a release tag; nothing is run.

    curl -sO https://raw.githubusercontent.com/LostRuins/koboldcpp/v1.124.2/src/llama-arch.cpp
    python3 -I tools/extract_archs.py llama-arch.cpp v1.124.2 > src-tauri/src/config/archs.json
"""
import json
import re
import sys


def main():
    path = sys.argv[1]
    version = sys.argv[2] if len(sys.argv) > 2 else None
    text = open(path, encoding="utf-8").read()
    names = re.findall(r'\{\s*LLM_ARCH_[A-Z0-9_]+\s*,\s*"([^"]+)"\s*\}', text)
    archs = sorted({n for n in names if n != "(unknown)"})
    json.dump({"source": "src/llama-arch.cpp", "version": version, "archs": archs}, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
