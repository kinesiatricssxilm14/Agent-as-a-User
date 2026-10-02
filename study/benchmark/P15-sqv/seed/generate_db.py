#!/usr/bin/env python3
"""Generate seed/bench.db with ~5000 Faker users + pinned benchmark rows."""

import random
import sqlite3
from pathlib import Path

from faker import Faker

fake = Faker()
random.seed(42)
fake.seed_instance(42)

OUT = Path(__file__).with_name("bench.db")
USER_COUNT = 5000

DEPTS = ["Engineering", "Design", "Product", "Marketing", "Sales", "Legal", "HR"]

PINNED_USERS = [
    (1, "Alice", "alice@ex.com", 30, "Engineering"),
    (2, "Bob", "bob@ex.com", 25, "Design"),
    (3, "Carol", "carol@ex.com", 35, "Engineering"),
    (4, "Dave", "dave@ex.com", 28, "Product"),
]

PINNED_USER_LAST = (5000, "Frank Moore", "frank.moore@ex.com", 66, "HR")

PINNED_PROJECTS = [
    (1, "Alpha", "active"),
    (2, "Beta", "complete"),
]

PROJECT_STATUSES = ["active", "complete", "paused", "planning"]


def main() -> None:
    if OUT.exists():
        OUT.unlink()

    conn = sqlite3.connect(OUT)
    cur = conn.cursor()
    cur.executescript(
        """
        CREATE TABLE users (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL,
            age INTEGER NOT NULL,
            dept TEXT NOT NULL
        );
        CREATE TABLE projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            status TEXT NOT NULL
        );
        """
    )

    cur.executemany(
        "INSERT INTO users (id, name, email, age, dept) VALUES (?, ?, ?, ?, ?)",
        PINNED_USERS,
    )

    used_emails = {row[2] for row in PINNED_USERS} | {PINNED_USER_LAST[2]}
    for user_id in range(5, USER_COUNT):
        name = fake.name()
        email = fake.unique.email()
        while email in used_emails:
            email = fake.unique.email()
        used_emails.add(email)
        cur.execute(
            "INSERT INTO users (id, name, email, age, dept) VALUES (?, ?, ?, ?, ?)",
            (
                user_id,
                name,
                email,
                random.randint(22, 65),
                random.choice(DEPTS),
            ),
        )

    cur.execute(
        "INSERT INTO users (id, name, email, age, dept) VALUES (?, ?, ?, ?, ?)",
        PINNED_USER_LAST,
    )

    cur.executemany(
        "INSERT INTO projects (id, name, status) VALUES (?, ?, ?)",
        PINNED_PROJECTS,
    )
    for project_id in range(3, 53):
        cur.execute(
            "INSERT INTO projects (id, name, status) VALUES (?, ?, ?)",
            (
                project_id,
                fake.catch_phrase(),
                random.choice(PROJECT_STATUSES),
            ),
        )

    conn.commit()
    users = cur.execute("SELECT COUNT(*) FROM users").fetchone()[0]
    projects = cur.execute("SELECT COUNT(*) FROM projects").fetchone()[0]
    conn.close()

    size_kb = OUT.stat().st_size // 1024
    print(f"Wrote {OUT} — users={users}, projects={projects}, size={size_kb}KB")


if __name__ == "__main__":
    main()
