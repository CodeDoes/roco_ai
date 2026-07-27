#!/bin/bash
# sccache wrapper — unsets CARGO_INCREMENTAL (sccache rejects it)
unset CARGO_INCREMENTAL
exec sccache "$@"
