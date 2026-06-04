from __future__ import annotations

import argparse
import os
from pathlib import Path

import uvicorn


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Dream Narrative Engine desktop backend")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--data-dir", default=os.getenv("DNE_DATA_DIR"))
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.data_dir:
        data_dir = Path(args.data_dir).expanduser().resolve()
        data_dir.mkdir(parents=True, exist_ok=True)
        os.environ["DNE_DATA_DIR"] = str(data_dir)

    uvicorn.run(
        "backend.app.main:app",
        host=args.host,
        port=args.port,
        reload=False,
        access_log=False,
    )


if __name__ == "__main__":
    main()
