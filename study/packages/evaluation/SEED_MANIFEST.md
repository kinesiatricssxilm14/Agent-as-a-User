# Seed English-only text

Benchmark English-only text **English-only text、English-only text、English-only text**。English-only text，English-only text fixture English-only text `seed/` English-only text。

## English-only text `SEED_ASSETS.md`

English-only text：`dockerfiles/Pxx-slug/SEED_ASSETS.md`（English-only text `bench50-pool/Pxx-slug/SEED_ASSETS.md`）

English-only text：`evaluation/templates/SEED_ASSETS.md.template`

## English-only text

| English-only text | English-only text |
|----|------|
| **Asset** | English-only text，English-only text `seed/employees.xlsx` |
| **Container path** | English-only text，English-only text `/bench/data/employees.xlsx` |
| **Used by tasks** | T01, T02… |
| **Origin** | `committed` / `generated-in-init` / **English-only text** |
| **Notes** | English-only text、English-only text、English-only text |

## English-only text

1. **committed（English-only text）** — English-only text/English-only text CSV/English-only text git English-only text，English-only text `seed/`，`init.sh` English-only text `cp` English-only text `unzip`。
2. **generated-in-init** — English-only text ≤10 English-only text（English-only text `git init` + 5 English-only text commit）English-only text `init.sh`。
3. **English-only text** — English-only text、English-only text、English-only text：English-only text MD English-only text **Preparation** English-only text，**English-only text**English-only text reviewer English-only text。

## init.sh English-only text seed/ English-only text

```
seed/
├── init.sh              # English-only text：English-only text、English-only text
├── employees.xlsx       # ★  committed fixture
├── repo-fixture.tar.gz  # ★  optional archive
└── config/
    └── feeds.opml       # ★  RSS OPML English-only text
```

`entrypoint.sh` English-only text `/bench/init.sh` English-only text `exec` TUI。

## English-only text Oracle English-only text

- **Seed English-only text**English-only text（English-only text）。
- **English-only text**English-only text run English-only text `bench-oracle gen-fingerprints` English-only text，English-only text **English-only text/English-only text** English-only text token。
- **English-only text**（English-only text [`ORACLE_SCRIPT.md`](./ORACLE_SCRIPT.md) §5）：
  1. **English-only text / oracle**：`{{file_content}}`、`load_fingerprints` → `$FP_*`
  2. **Seed materialize**：English-only text staging English-only text → `docker cp` English-only text（**English-only text**；English-only text seed English-only text）
- Agent **English-only text**English-only text（T02 English-only text）English-only text seed_inject；English-only text Agent English-only text oracle English-only text。

## English-only text

- [ ] `SEED_ASSETS.md` English-only text `seed/` English-only text
- [ ] English-only text「English-only text init.sh English-only text curl English-only text」English-only text
- [ ] Docker `COPY seed/` English-only text committed English-only text
- [ ] English-only text seed（RSS URL English-only text）English-only text note.md English-only text

## English-only text

- [examples/P26-xleak/SEED_ASSETS.md](./examples/P26-xleak/SEED_ASSETS.md)
