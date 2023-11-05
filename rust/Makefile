local:
	RUST_LOG=info cargo run

watch:
	cargo watch -- RUST_LOG=info cargo run

release:
	cargo +nightly build -Zgitoxide --release

docker:
	docker build -t azos-keeper .