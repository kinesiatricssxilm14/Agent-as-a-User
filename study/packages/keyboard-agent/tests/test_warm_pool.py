from __future__ import annotations

import json
from pathlib import Path

from keyboard_agent.human.warm_pool import (
    SlotState,
    WarmPool,
    WarmPoolConfig,
    clone_project_for_slot,
    load_project_task_ids,
    port_stride_for_pool,
    resolve_warm_pool_config,
    sync_slot_from_source,
    warm_pool_policy,
)


def test_load_project_task_ids_all(tmp_path):
    project = tmp_path / "P05-elio"
    project.mkdir()
    (project / "bench.spec.json").write_text(
        json.dumps({"tasks": [{"id": "T01"}, {"id": "T02"}, {"id": "T04"}]}),
        encoding="utf-8",
    )
    assert load_project_task_ids(project, all_tasks=True) == ["T01", "T02", "T04"]
    assert load_project_task_ids(project, task_ids=["T02", "T01"]) == ["T02", "T01"]


def test_port_stride_for_pool():
    assert port_stride_for_pool(3) == 8
    assert port_stride_for_pool(1) == 6


def test_clone_project_for_slot_isolated_oracle(tmp_path):
    src = tmp_path / "P05-elio"
    src.mkdir()
    (src / "Dockerfile").write_text("FROM ubuntu\n", encoding="utf-8")
    (src / "bench.spec.json").write_text("{}", encoding="utf-8")
    (src / "entrypoint.sh").write_text("#!/bin/bash\n", encoding="utf-8")
    (src / "seed").mkdir()
    (src / "seed" / "init.sh").write_text("echo hi\n", encoding="utf-8")
    (src / "oracle").mkdir()
    (src / "oracle" / "T01.sh").write_text("echo pass\n", encoding="utf-8")
    oracle = src / ".oracle"
    oracle.mkdir()
    (oracle / "T01_fingerprints.json").write_text("{}", encoding="utf-8")

    slot = tmp_path / "slot-0"
    clone_project_for_slot(src, slot)

    assert (slot / "Dockerfile").is_file()
    assert not (slot / "Dockerfile").is_symlink()
    assert (slot / "seed").is_dir()
    assert not (slot / "seed").is_symlink()
    assert (slot / "bench.spec.json").is_symlink()
    assert (slot / "oracle").is_symlink()
    assert (slot / "seed" / "init.sh").read_text(encoding="utf-8") == "echo hi\n"
    assert (slot / ".oracle").is_dir()
    assert not (slot / ".oracle").is_symlink()
    assert (slot / ".oracle" / "T01_fingerprints.json").is_file()
    assert not (slot / ".oracle" / "current_session.json").exists()
    assert (slot / ".oracle" / "fingerprint_scope").read_text(encoding="utf-8").strip() == "P05-elio"


def test_sync_slot_from_source_refreshes_fingerprints(tmp_path):
    src = tmp_path / "P12-tredis"
    src.mkdir()
    oracle = src / ".oracle"
    oracle.mkdir()
    (oracle / "T01_fingerprints.json").write_text(
        '{"file_content": "fc_01251a7a"}',
        encoding="utf-8",
    )
    slot = tmp_path / "slot-0"
    slot.mkdir()
    (slot / ".oracle").mkdir()
    (slot / ".oracle" / "T01_fingerprints.json").write_text(
        '{"file_content": "fc_p12_t01_c32274ba_157ae17e2744"}',
        encoding="utf-8",
    )
    staged = slot / ".oracle" / "staged_seed" / "T01"
    staged.mkdir(parents=True)
    (staged / "init.sh").write_text("old", encoding="utf-8")

    sync_slot_from_source(src, slot, task_id="T01")

    assert (
        (slot / ".oracle" / "T01_fingerprints.json").read_text(encoding="utf-8")
        == '{"file_content": "fc_01251a7a"}'
    )
    assert not staged.exists()
    assert (
        (slot / ".oracle" / "fingerprint_scope").read_text(encoding="utf-8").strip()
        == "P12-tredis"
    )


def test_pool_status_format(tmp_path):
    project = tmp_path / "P05-elio"
    project.mkdir()
    pool = WarmPool(
        source_project=project,
        bench_script=tmp_path / "bench.sh",
        task_id="T02",
        pool_root=tmp_path / "pool",
        config=WarmPoolConfig(pool_size=2),
    )
    pool.slots[0].state = SlotState.READY
    pool.slots[1].state = SlotState.WARMING
    status = pool._format_pool_status()
    assert "1/2 English-only text" in status
    assert "1 English-only text" in status


def test_warm_pool_default_root(tmp_path):
    project = tmp_path / "P05-elio"
    project.mkdir()
    root = WarmPool.default_pool_root(tmp_path / "runs", project, "T02")
    assert root == tmp_path / "runs" / ".warm_pool" / "P05-elio-T02"


def test_p11_warm_pool_policy(tmp_path):
    project = tmp_path / "P11-dusk"
    project.mkdir()
    (project / "bench.spec.json").write_text(
        json.dumps(
            {
                "project_id": "P11",
                "warm_pool": {"max_slots": 1, "single_concurrent_warm": True},
                "tasks": [{"id": "T01"}],
            }
        ),
        encoding="utf-8",
    )
    policy = warm_pool_policy(project)
    assert policy.max_slots == 1
    assert policy.single_concurrent_warm is True
    config, _ = resolve_warm_pool_config(
        source_project=project,
        output_dir=tmp_path / "runs",
        pool_size=3,
        base_port=8765,
        rebuild=False,
        no_build=True,
    )
    assert config.pool_size == 1
    assert config.warm_lock_path == tmp_path / "runs" / ".warm_pool" / "P11-dusk.warm.lock"


from keyboard_agent.human.warm_pool import WarmPoolConfig  # noqa: E402
