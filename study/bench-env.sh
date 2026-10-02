#!/usr/bin/env bash
# Shared benchmark path resolution for curated-study-kit scripts.
resolve_bench_locale() {
  local locale="${BENCH_LOCALE:-cn}"
  case "$locale" in
    cn|en) ;;
    *)
      echo "Invalid BENCH_LOCALE=$locale (expected cn or en)" >&2
      return 1
      ;;
  esac
  printf '%s' "$locale"
}

resolve_benchmark_root() {
  if [ -n "${BENCH_SUITE_ROOT:-}" ] && [ -d "${BENCH_SUITE_ROOT}" ]; then
    printf '%s' "$BENCH_SUITE_ROOT"
    return 0
  fi
  local locale
  locale="$(resolve_bench_locale)" || return 1
  printf '%s' "$KIT_DIR/benchmark_${locale}"
}
