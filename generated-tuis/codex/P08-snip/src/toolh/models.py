from dataclasses import dataclass


@dataclass(frozen=True)
class Snippet:
    id: int
    title: str
    language: str
    tags: str
    description: str
    code: str
    created_at: str
    updated_at: str

