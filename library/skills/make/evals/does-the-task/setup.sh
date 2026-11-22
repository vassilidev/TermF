#!/bin/sh
set -e
printf '.PHONY: test lint\n\ntest:\n\tgo test ./...\n\nlint:\n\tgo vet ./...\n' > Makefile
printf 'module example.com/acme\n\ngo 1.27\n' > go.mod
