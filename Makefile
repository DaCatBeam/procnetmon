SHELL := /bin/bash

.PHONY: run-dev run-tests run-fmt run-mutants run-fmt record-demo

run-dev:
	docker rm procnetmon || exit 0;
	docker build --no-cache --platform linux/amd64 -t procnetmon_dev_image .;
	docker run -it --name procnetmon procnetmon_dev_image;

run-tests:
	cargo test;

debug-tests:
	cargo test -- --show-output;

run-mutants:
	cargo mutants;

run-fmt:
	cargo fmt;

record-demo:
	cargo build --release --bins --examples;
	vhs docs/demo.tape;
