all: clean main

clean:
	rm -rf receiver/target
	rm -f receiver-release

main:
	cd receiver && cargo build --release
	cp receiver/target/release/receiver receiver-release
