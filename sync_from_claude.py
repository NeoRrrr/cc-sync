#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""Compatibility wrapper for old configs. Use sync_agents.py instead."""

from sync_agents import main


if __name__ == "__main__":
    raise SystemExit(main())
