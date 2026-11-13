"""Progress notifications shared by CLI, GUI and unit tests."""
from __future__ import annotations
import json
import sys
import threading
import time

class Progress:
    def __init__(self, bar=False, json_events=False, stream=None):
        self.bar = bar
        self.json_events = json_events
        self.stream = stream or sys.stderr
        self.lock = threading.Lock()
        self.last = 0.0
        self.current = 0
        self.total = 1

    def emit(self, stage, message, *, completed=None, total=None, repository=None, **data):
        with self.lock:
            if completed is not None:
                self.current = completed
            if total is not None:
                self.total = max(1, total)
            pct = min(100, max(0, round(100 * self.current / self.total)))
            event = {"type": "progress", "stage": stage, "message": message,
                     "completed": self.current, "total": self.total, "percent": pct,
                     "repository": repository, **data}
            if self.json_events:
                self.stream.write(json.dumps(event, ensure_ascii=False) + "\n")
            elif self.bar:
                width = 28
                blocks = round(width * pct / 100)
                self.stream.write("\r[" + "#" * blocks + "-" * (width-blocks)
                                  + f"] {pct:3}% {stage}: {message[:58]:58}\n")
            else:
                self.stream.write(f"[{stage}] {message}\n")
            self.stream.flush()
            return event
